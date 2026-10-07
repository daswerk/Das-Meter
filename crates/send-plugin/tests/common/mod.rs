//! A minimal in-process CLAP host around the Send Plugin, built with
//! `clack-host`, so tests and the benchmark exercise the real `process()`.

#![allow(dead_code)] // each test binary uses a different part

use clack_extensions::gui::{GuiApiType, GuiConfiguration, PluginGui};
use clack_extensions::state::PluginState;
use clack_host::events::event_types::TransportEvent;
use clack_host::prelude::*;
use clack_plugin::entry::SinglePluginEntry;
use dasmeter_send::plugin::{PLUGIN_ID, SendPlugin, use_table};

pub struct TestHostShared;

impl SharedHandler<'_> for TestHostShared {
    fn request_restart(&self) {}
    fn request_process(&self) {}
    fn request_callback(&self) {}
}

pub struct TestHost;

impl HostHandlers for TestHost {
    type Shared<'a> = TestHostShared;
    type MainThread<'a> = ();
    type AudioProcessor<'a> = ();
}

/// One Send Plugin instance, activated and processing.
pub struct Instance {
    // Field order matters: the processor goes before the instance, the instance before the entry.
    processor: Option<StartedPluginAudioProcessor<TestHost>>,
    instance: PluginInstance<TestHost>,
    _entry: PluginEntry,
    ports: (AudioPorts, AudioPorts),
}

impl Instance {
    /// Creates an instance on table `table`, restores `state` if given, and activates it.
    pub fn new(table: &str, state: Option<&[u8]>, max_frames: u32) -> Instance {
        use_table(table);
        let entry =
            PluginEntry::load_from_clack::<SinglePluginEntry<SendPlugin>>(c"/test/das-meter.clap")
                .expect("load the entry");
        let host_info =
            HostInfo::new("Test host", "Das Werk", "https://example.com", "1.0").unwrap();
        let id = std::ffi::CString::new(PLUGIN_ID).unwrap();
        let instance =
            PluginInstance::<TestHost>::new(|_| TestHostShared, |_| (), &entry, &id, &host_info)
                .expect("instantiate");
        let mut this = Instance {
            processor: None,
            instance,
            _entry: entry,
            ports: (
                AudioPorts::with_capacity(2, 1),
                AudioPorts::with_capacity(2, 1),
            ),
        };
        if let Some(state) = state {
            this.load(state);
        }
        this.activate(max_frames);
        this
    }

    fn activate(&mut self, max_frames: u32) {
        let config = PluginAudioConfiguration {
            sample_rate: 48_000.0,
            min_frames_count: 1,
            max_frames_count: max_frames,
        };
        let stopped = self.instance.activate(|_, _| (), config).expect("activate");
        let Ok(started) = stopped.start_processing() else {
            panic!("start processing");
        };
        self.processor = Some(started);
    }

    /// Runs `process()` on one stereo block, in place of a host's audio thread.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32], out: &mut [Vec<f32>; 2]) {
        self.process_at(left, right, out, None);
    }

    /// [`Instance::process`] with the host's transport at the block's first frame.
    pub fn process_at(
        &mut self,
        left: &mut [f32],
        right: &mut [f32],
        out: &mut [Vec<f32>; 2],
        transport: Option<&TransportEvent>,
    ) {
        let (in_ports, out_ports) = &mut self.ports;
        let inputs = in_ports.with_input_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_input_only(
                [left, right].into_iter().map(InputChannel::variable),
            ),
        }]);
        let [out_left, out_right] = out;
        let mut outputs = out_ports.with_output_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_output_only(
                [out_left.as_mut_slice(), out_right.as_mut_slice()].into_iter(),
            ),
        }]);
        self.processor
            .as_mut()
            .expect("processing")
            .process(
                &inputs,
                &mut outputs,
                &InputEvents::empty(),
                &mut OutputEvents::void(),
                None,
                transport,
            )
            .expect("process");
    }

    /// A host's transport: `tempo` BPM in `signature`, at `beats` quarter notes
    /// into a bar that began at `bar_start`.
    pub fn transport(
        tempo: f64,
        beats: f64,
        bar_start: f64,
        signature: (u16, u16),
        playing: bool,
    ) -> TransportEvent {
        use clack_host::events::event_types::TransportFlags;
        use clack_host::events::{EventFlags, EventHeader};
        use clack_host::utils::{BeatTime, SecondsTime};
        let mut flags = TransportFlags::HAS_TEMPO
            | TransportFlags::HAS_BEATS_TIMELINE
            | TransportFlags::HAS_TIME_SIGNATURE;
        if playing {
            flags |= TransportFlags::IS_PLAYING;
        }
        TransportEvent {
            header: EventHeader::new_core(0, EventFlags::empty()),
            flags,
            song_pos_beats: BeatTime::from_float(beats),
            song_pos_seconds: SecondsTime::from_float(beats * 60.0 / tempo),
            tempo,
            tempo_inc: 0.0,
            loop_start_beats: BeatTime::from_int(0),
            loop_end_beats: BeatTime::from_int(0),
            loop_start_seconds: SecondsTime::from_int(0),
            loop_end_seconds: SecondsTime::from_int(0),
            bar_start: BeatTime::from_float(bar_start),
            bar_number: 0,
            time_signature_numerator: signature.0,
            time_signature_denominator: signature.1,
        }
    }

    /// The plugin's saved state, as a host would store it in the project.
    pub fn save(&mut self) -> Vec<u8> {
        let handle = self.instance.plugin_handle();
        let state = handle
            .get_extension::<PluginState>()
            .expect("state extension");
        let mut bytes = Vec::new();
        state.save(&handle, &mut bytes).expect("save");
        bytes
    }

    /// Asks the plugin's GUI extension what a host would before opening the
    /// window. Doesn't open one: that needs a parent window.
    pub fn gui(&mut self) -> Gui {
        let handle = self.instance.plugin_handle();
        let gui = handle.get_extension::<PluginGui>().expect("gui extension");
        let platform = GuiApiType::default_for_current_platform().expect("a windowing API");
        let embedded = GuiConfiguration {
            api_type: platform,
            is_floating: false,
        };
        let floating = GuiConfiguration {
            api_type: platform,
            is_floating: true,
        };
        let preferred = gui
            .get_preferred_api(&handle)
            .map(|c| (c.api_type == platform, c.is_floating));
        Gui {
            embedded: gui.is_api_supported(&handle, embedded),
            floating: gui.is_api_supported(&handle, floating),
            preferred,
            size: gui.get_size(&handle).map(|s| (s.width, s.height)),
            can_resize: gui.can_resize(&handle),
            takes_scale: gui.set_scale(&handle, 2.0).is_ok(),
        }
    }

    fn load(&mut self, bytes: &[u8]) {
        let handle = self.instance.plugin_handle();
        let state = handle
            .get_extension::<PluginState>()
            .expect("state extension");
        state.load(&handle, &mut &bytes[..]).expect("load");
    }
}

/// What the plugin tells a host about its window.
#[derive(Debug)]
pub struct Gui {
    pub embedded: bool,
    pub floating: bool,
    /// (the platform's API, floating)
    pub preferred: Option<(bool, bool)>,
    pub size: Option<(u32, u32)>,
    pub can_resize: bool,
    pub takes_scale: bool,
}

impl Drop for Instance {
    fn drop(&mut self) {
        if let Some(processor) = self.processor.take() {
            self.instance.deactivate(processor.stop_processing());
        }
    }
}
