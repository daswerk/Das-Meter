//! The Das-Meter app: turns the app core's scene into windows and Meters.
//!
//! The shell is thin: it feeds System Capture or Send Plugin audio and window
//! events into the app core, draws the scene when the core says so, and
//! otherwise sleeps.

mod benchmark;
mod capture;
mod gpu;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod main_menu;
#[cfg(target_os = "macos")]
mod maintenance;
mod meters;
mod painter;
#[cfg(target_os = "macos")]
mod plugin_install;
mod preset_files;
mod send_plugins;
mod sharing;
mod shell;
mod snapshot;
mod theme_files;
mod ui;
#[cfg(target_os = "macos")]
mod updates;

use std::sync::atomic::AtomicU8;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use dasmeter_core::{AppCore, Decision, Event, ListenTo, MeterState, MeterView};
use rtrb::Consumer;

use capture::{CaptureMessage, SystemCapture, Waker};
use send_plugins::SendPluginInput;

fn version_line() -> String {
    format!("Das-Meter {}", env!("CARGO_PKG_VERSION"))
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--version") => println!("{}", version_line()),
        // Hidden: the performance budget's scenes, measured in a real window.
        Some("--benchmark") => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            benchmark::run(&args);
        }
        // Hidden: System Capture into the app core, readings printed once a second.
        Some("--print-readings") => print_readings(),
        // Hidden: two Loudness Meters on the first two Send Plugins, printed once a second.
        Some("--print-send-plugins") => print_send_plugins(),
        // Hidden: a generated signal through the core, drawn offscreen to a PPM
        // image, optionally with a Meter menu or the settings panel open.
        Some("--render-snapshot") => {
            let path = std::env::args().nth(2).unwrap_or("snapshot.ppm".into());
            let open = std::env::args().nth(3);
            if let Err(error) = snapshot::render(&path, open.as_deref()) {
                eprintln!("snapshot: {error}");
                std::process::exit(1);
            }
        }
        _ => shell::run(),
    }
}

/// System Capture feeding the app core: the active Source's ring and the capture thread's messages.
pub(crate) struct Audio {
    messages: mpsc::Receiver<CaptureMessage>,
    ring: Option<Consumer<f32>>,
    waker: Arc<Waker>,
    /// How often capture wakes the main thread: `capture::DRAWING` and so on.
    pub(crate) pace: Arc<AtomicU8>,
    _capture: SystemCapture,
}

impl Audio {
    pub(crate) fn start(wake: impl Fn() + Send + Sync + 'static) -> Audio {
        let waker = Waker::new(wake);
        let pace = Arc::new(AtomicU8::new(capture::DRAWING));
        let (capture, messages) = SystemCapture::start(waker.clone(), pace.clone());
        Audio {
            messages,
            ring: None,
            waker,
            pace,
            _capture: capture,
        }
    }

    /// Feeds everything captured so far into the core, in order.
    pub(crate) fn pump(&mut self, core: &mut AppCore, now: Duration) {
        self.waker.clear();
        loop {
            if let Some(ring) = &mut self.ring {
                let available = ring.slots();
                if let Ok(chunk) = ring.read_chunk(available) {
                    let (first, second) = chunk.as_slices();
                    core.handle(Event::Audio(first), now);
                    core.handle(Event::Audio(second), now);
                    chunk.commit_all();
                }
            }
            // The previous ring is complete once its successor is announced.
            match self.messages.try_recv() {
                Ok(CaptureMessage::Started { sample_rate, audio }) => {
                    core.handle(Event::CaptureStarted { sample_rate }, now);
                    self.ring = Some(audio);
                }
                Ok(CaptureMessage::Failed(reason)) => {
                    core.handle(Event::CaptureFailed(&reason), now);
                    self.ring = None;
                }
                Err(_) => break,
            }
        }
    }
}

/// Runs System Capture into the app core without a window and prints the
/// Loudness Meter's readings once a second, for checks against a reference.
fn print_readings() {
    let start = Instant::now();
    let mut core = AppCore::new();
    let mut audio = Audio::start(|| {});
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let now = start.elapsed();
        audio.pump(&mut core, now);
        if core.decide(now) != Decision::Draw {
            continue;
        }
        let Some(scene) = core.scene() else { continue };
        let notes: Vec<_> = scene.notes.iter().map(|note| note.text()).collect();
        let loudness = scene.windows[0]
            .meters
            .iter()
            .find_map(|meter| match &meter.state {
                MeterState::Live(MeterView::Loudness { display, .. }) => Some(Ok(*display)),
                MeterState::Live(_) => None,
                other => Some(Err(other.clone())),
            });
        match loudness {
            Some(Ok(d)) => {
                let show = |level: dasmeter_core::Level| {
                    level
                        .db()
                        .map_or_else(|| "-inf".to_owned(), |db| format!("{db:.1}"))
                };
                println!(
                    "{:>6.1}s {} Hz  M {:>6}  S {:>6}  I {:>6} LUFS  LRA {:>5}  TP {:>6}  peak L {:>6} R {:>6}  RMS L {:>6} R {:>6}  {}",
                    now.as_secs_f64(),
                    d.sample_rate,
                    show(d.momentary),
                    show(d.short_term),
                    show(d.integrated),
                    show(d.range),
                    show(d.true_peak_max),
                    show(d.left.peak),
                    show(d.right.peak),
                    show(d.left.rms),
                    show(d.right.rms),
                    notes.join(", "),
                );
            }
            other => println!("{:>6.1}s {other:?} {}", now.as_secs_f64(), notes.join(", ")),
        }
    }
}

/// Listens to Send Plugins without a window: two Loudness Meters, the first
/// picking the first Send Plugin listed (by name) and the second the second,
/// printed once a second. For checking two DAW tracks against each other.
fn print_send_plugins() {
    use dasmeter_core::{LoudnessMeterSettings, MeterSettings, SendPlugin};

    let start = Instant::now();
    let mut core = AppCore::with_meters(vec![
        MeterSettings::Loudness(LoudnessMeterSettings::default()),
        MeterSettings::Loudness(LoudnessMeterSettings::default()),
    ]);
    core.handle(Event::SetListenTo(ListenTo::SendPlugins), Duration::ZERO);
    for meter in 0..2 {
        core.handle(
            Event::ShowSourceLabel { meter, shown: true },
            Duration::ZERO,
        );
    }
    let mut input = SendPluginInput::new();
    let mut picked = false;
    let mut printed = Duration::ZERO;
    loop {
        std::thread::sleep(dasmeter_core::DEFAULT_FRAME_INTERVAL);
        let now = start.elapsed();
        input.pump(&mut core, now, start + now);
        if !picked {
            let mut listed: Vec<SendPlugin> = input.listed();
            listed.retain(|p| p.state != dasmeter_core::SendPluginState::Gone);
            listed.sort_by(|a, b| a.name.cmp(&b.name));
            if listed.len() >= 2 {
                for (meter, plugin) in listed.iter().take(2).enumerate() {
                    core.handle(
                        Event::PickSendPlugin {
                            meter,
                            id: plugin.id,
                        },
                        now,
                    );
                }
                picked = true;
            }
        }
        core.decide(now);
        if now < printed + Duration::from_secs(1) {
            continue;
        }
        printed = now;
        let Some(scene) = core.scene() else { continue };
        let line: Vec<String> = scene.windows[0]
            .meters
            .iter()
            .map(|meter| {
                let source = meter.source.as_ref().map_or("-", |s| s.name.as_str());
                match &meter.state {
                    MeterState::Live(MeterView::Loudness { display, .. }) => format!(
                        "{source}: M {} LUFS, peak {} dBFS, {} Hz",
                        display
                            .momentary
                            .db()
                            .map_or("-inf".into(), |db| format!("{db:.1}")),
                        display
                            .left
                            .peak
                            .db()
                            .map_or("-inf".into(), |db| format!("{db:.1}")),
                        display.sample_rate
                    ),
                    other => format!("{source}: {other:?}"),
                }
            })
            .collect();
        println!("{:>6.1}s  {}", now.as_secs_f64(), line.join("   |   "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_line_names_the_app() {
        assert!(version_line().starts_with("Das-Meter "));
    }
}
