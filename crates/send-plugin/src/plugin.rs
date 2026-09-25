//! The CLAP plugin: glue between the host and [`Link`] / [`AudioSend`].

use std::cell::RefCell;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use clack_extensions::audio_ports::{
    AudioPortFlags, AudioPortInfo, AudioPortInfoWriter, AudioPortType, PluginAudioPorts,
    PluginAudioPortsImpl,
};
use clack_extensions::state::{PluginState, PluginStateImpl};
use clack_extensions::timer::{HostTimer, PluginTimer, PluginTimerImpl, TimerId};
use clack_extensions::track_info::{
    HostTrackInfo, PluginTrackInfo, PluginTrackInfoImpl, TrackInfoBuffer,
};
use clack_plugin::prelude::*;
use clack_plugin::stream::{InputStream, OutputStream};
use dasmeter_transport::{HEARTBEAT_INTERVAL, TABLE_NAME};

use crate::identity::{StdRandom, Track};
use crate::link::{Link, Status};
use crate::send::{AudioSend, Block, pass_through};

/// The plugin's CLAP ID.
pub const PLUGIN_ID: &str = "com.daswerk.das-meter.send";

/// Table the plugin claims its slot in. Tests point it elsewhere with [`use_table`].
static TABLE: Mutex<Option<String>> = Mutex::new(None);

/// Makes Send Plugins created from now on use another table. For tests.
#[doc(hidden)]
pub fn use_table(name: &str) {
    *TABLE.lock().unwrap_or_else(|e| e.into_inner()) = Some(name.to_owned());
}

fn table_name() -> String {
    TABLE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_else(|| TABLE_NAME.to_owned())
}

pub struct SendPlugin;

impl Plugin for SendPlugin {
    type AudioProcessor<'a> = Processor;
    type Shared<'a> = Shared;
    type MainThread<'a> = MainThread<'a>;

    fn declare_extensions(builder: &mut PluginExtensions<Self>, _shared: Option<&Shared>) {
        builder
            .register::<PluginAudioPorts>()
            .register::<PluginState>()
            .register::<PluginTimer>()
            .register::<PluginTrackInfo>();
    }
}

impl DefaultPluginFactory for SendPlugin {
    fn get_descriptor() -> PluginDescriptor {
        use clack_plugin::plugin::features::{ANALYZER, AUDIO_EFFECT, MONO, STEREO, UTILITY};
        PluginDescriptor::new(PLUGIN_ID, "Das-Meter Send")
            .with_vendor("Das Werk")
            .with_url("https://github.com/daswerk/Das-Meter")
            .with_version(env!("CARGO_PKG_VERSION"))
            .with_description("Sends this track's audio to the Das-Meter app")
            .with_features([AUDIO_EFFECT, ANALYZER, UTILITY, STEREO, MONO])
    }

    fn new_shared(_host: HostSharedHandle<'_>) -> Result<Shared, PluginError> {
        Ok(Shared {
            mono: Arc::new(AtomicBool::new(false)),
        })
    }

    fn new_main_thread<'a>(
        host: HostMainThreadHandle<'a>,
        shared: &'a Shared,
    ) -> Result<MainThread<'a>, PluginError> {
        let timer_ext = host.get_extension::<HostTimer>();
        let timer = timer_ext.and_then(|timer| {
            timer
                .register_timer(&host, HEARTBEAT_INTERVAL.as_millis() as u32)
                .ok()
        });
        let main = MainThread {
            host,
            shared,
            timer: timer_ext.zip(timer),
            track_info: host.get_extension::<HostTrackInfo>(),
            state: RefCell::new(State {
                link: Link::with(&table_name(), StdRandom),
                active: false,
            }),
        };
        if main.timer.is_none() {
            main.state.borrow_mut().link.use_own_heartbeat();
        }
        main.read_track();
        Ok(main)
    }
}

/// Shared between the audio and main threads.
pub struct Shared {
    /// Whether the audio arrives as one channel. Set by the audio thread.
    mono: Arc<AtomicBool>,
}

impl PluginShared<'_> for Shared {}

pub struct MainThread<'a> {
    host: HostMainThreadHandle<'a>,
    shared: &'a Shared,
    timer: Option<(HostTimer, TimerId)>,
    track_info: Option<HostTrackInfo>,
    state: RefCell<State>,
}

struct State {
    link: Link,
    active: bool,
}

impl MainThread<'_> {
    /// Reads the track's name, colour and channel count from the host.
    fn read_track(&self) {
        let Some(track_info) = self.track_info else {
            return;
        };
        let mut host = self.host;
        let mut buffer = TrackInfoBuffer::new();
        let Some(info) = track_info.get(&mut host, &mut buffer) else {
            return;
        };
        let track = Track {
            name: info
                .name()
                .map(|name| String::from_utf8_lossy(name).into_owned()),
            colour: info
                .color()
                .filter(|c| c.alpha != 0)
                .map(|c| (u32::from(c.red) << 16) | (u32::from(c.green) << 8) | u32::from(c.blue)),
        };
        let mono = info.audio_channel_count() == Some(1);
        let mut state = self.state.borrow_mut();
        state.link.set_track(track);
        if mono {
            state.link.set_mono(true);
        }
    }

    /// The connection status, for the window.
    pub fn status(&self) -> Status {
        self.state.borrow().link.status()
    }
}

impl Drop for MainThread<'_> {
    fn drop(&mut self) {
        if let Some((timer, id)) = self.timer {
            let _ = timer.unregister_timer(&self.host, id);
        }
    }
}

impl<'a> PluginMainThread<'a, Shared> for MainThread<'a> {}

impl PluginTimerImpl for MainThread<'_> {
    fn on_timer(&self, _timer_id: TimerId) {
        let mut state = self.state.borrow_mut();
        state.link.set_mono(self.shared.mono.load(Relaxed));
        state.link.tick(Instant::now());
    }
}

impl PluginTrackInfoImpl for MainThread<'_> {
    fn changed(&self) {
        self.read_track();
    }
}

impl PluginStateImpl for MainThread<'_> {
    fn save(&self, output: &mut OutputStream) -> Result<(), PluginError> {
        let bytes = self.state.borrow().link.save();
        output.write_all(&bytes)?;
        Ok(())
    }

    fn load(&self, input: &mut InputStream) -> Result<(), PluginError> {
        let mut bytes = Vec::new();
        input.read_to_end(&mut bytes)?;
        let mut state = self.state.borrow_mut();
        let reclaimed = state
            .link
            .load(&bytes)
            .map_err(|_| PluginError::Message("unreadable Das-Meter Send state"))?;
        if reclaimed.is_some() && state.active {
            // The slot changed under the audio thread; reactivate to pick up the new one.
            self.host.shared().request_restart();
        }
        Ok(())
    }
}

impl PluginAudioPortsImpl for MainThread<'_> {
    fn count(&self, _is_input: bool) -> u32 {
        1
    }

    fn get(&self, index: u32, is_input: bool, writer: &mut AudioPortInfoWriter) {
        if index != 0 {
            return;
        }
        writer.set(&AudioPortInfo {
            id: ClapId::new(0),
            name: if is_input { b"Input" } else { b"Output" },
            channel_count: 2,
            flags: AudioPortFlags::IS_MAIN,
            port_type: Some(AudioPortType::STEREO),
            in_place_pair: Some(ClapId::new(0)),
        });
    }
}

/// The audio thread's state.
pub struct Processor {
    send: AudioSend,
}

impl<'a> PluginAudioProcessor<'a, Shared, MainThread<'a>> for Processor {
    fn activate(
        _host: HostAudioProcessorHandle<'a>,
        main_thread: &MainThread<'a>,
        shared: &'a Shared,
        audio_config: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        let mut state = main_thread.state.borrow_mut();
        let writer = state.link.activate(audio_config.sample_rate.round() as u32);
        state.active = true;
        Ok(Processor {
            send: AudioSend::new(writer, shared.mono.clone()),
        })
    }

    fn process(
        &mut self,
        _process: Process,
        mut audio: Audio,
        _events: Events,
    ) -> Result<ProcessStatus, PluginError> {
        self.process_audio(&mut audio);
        Ok(ProcessStatus::ContinueIfNotQuiet)
    }

    fn deactivate(self, main_thread: &MainThread<'a>) {
        main_thread.state.borrow_mut().active = false;
    }
}

impl Processor {
    /// Passes the main port through unchanged and sends it on. Other ports and
    /// 64-bit buffers pass through without being sent.
    fn process_audio(&mut self, audio: &mut Audio) {
        let mut sent = false;
        for index in 0..audio.port_pair_count() {
            let Some(mut pair) = audio.port_pair(index) else {
                continue;
            };
            let Ok(channels) = pair.channels() else {
                continue;
            };
            let Some(mut channels) = channels.into_f32() else {
                continue; // 64-bit only: we declare no 64-bit support, so hosts don't send it
            };
            let mut inputs: [&[f32]; 2] = [&[], &[]];
            let count = channels.channel_pair_count();
            for i in 0..count {
                let input: Option<&[f32]> = match channels.channel_pair(i) {
                    Some(ChannelPair::InputOutput(input, output)) => {
                        pass_through(input, output);
                        Some(input)
                    }
                    Some(ChannelPair::InPlace(buffer)) => Some(buffer),
                    Some(ChannelPair::InputOnly(input)) => Some(input),
                    Some(ChannelPair::OutputOnly(output)) => {
                        output.fill(0.0);
                        None
                    }
                    None => None,
                };
                if let (Some(input), Some(slot)) = (input, inputs.get_mut(i)) {
                    *slot = input;
                }
            }
            let count = count.min(2);
            if index == 0 && !sent {
                sent = true;
                match count {
                    0 => {}
                    1 => self.send.send(Block::Mono(inputs[0])),
                    _ => self.send.send(Block::Stereo(inputs[0], inputs[1])),
                }
            }
        }
    }
}

clack_export_entry!(SinglePluginEntry<SendPlugin>);
