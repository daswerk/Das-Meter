//! A Spectrum's Overlay Source and its collisions (#99): a second Send
//! Plugin on the same graph, marked where both are loud and close in level.

use std::time::Duration;

use dasmeter_analysis::signals::{both, sine};
use dasmeter_core::{
    AppCore, Event, ListenTo, MeterKind, MeterSettings, MeterState, MeterView, SendPlugin,
    SendPluginState, SpectrumOverlay,
};

const RATE: u32 = 48_000;
const KICK: u64 = 7;
const BASS: u64 = 8;

fn plugin(id: u64, name: &str, colour: u32) -> SendPlugin {
    SendPlugin {
        id,
        name: name.to_owned(),
        colour,
        mono: false,
        sample_rate: RATE,
        state: SendPluginState::Live,
        outdated: false,
        host_pid: 1,
    }
}

struct App {
    core: AppCore,
    now: Duration,
}

impl App {
    /// One Spectrum showing Kick, with Bass as its Overlay Source.
    fn new() -> App {
        let mut app = App {
            core: AppCore::with_meters(vec![MeterSettings::default_of(MeterKind::Spectrum)]),
            now: Duration::ZERO,
        };
        let listed = vec![
            plugin(KICK, "Kick", 0xff_40_20),
            plugin(BASS, "Bass", 0x20_80_ff),
        ];
        app.core.handle(Event::StartListening, app.now);
        app.core
            .handle(Event::SetListenTo(ListenTo::SendPlugins), app.now);
        app.core.handle(Event::SendPlugins(&listed), app.now);
        app.core
            .handle(Event::PickSendPlugin { meter: 0, id: KICK }, app.now);
        app.core.handle(
            Event::PickOverlay {
                meter: 0,
                id: Some(BASS),
            },
            app.now,
        );
        app
    }

    /// Kick plays `kick`, Bass plays `bass` (Hz, −12 dBFS), for a second.
    fn play(&mut self, kick: f64, bass: f64) {
        let kick = both(&sine(RATE, kick, -12.0, 0.0, RATE as usize));
        let bass = both(&sine(RATE, bass, -12.0, 0.0, RATE as usize));
        for (k, b) in kick.chunks(1024).zip(bass.chunks(1024)) {
            self.now += Duration::from_secs_f64(512.0 / f64::from(RATE));
            for (id, frames) in [(BASS, b), (KICK, k)] {
                if self.core.listened().contains(&id) {
                    self.core.handle(
                        Event::SendPluginAudio {
                            id,
                            frames,
                            timing: None,
                        },
                        self.now,
                    );
                }
            }
            self.core.decide(self.now);
        }
    }

    fn overlay(&mut self) -> (SpectrumOverlay, Vec<f32>) {
        self.now += Duration::from_millis(40);
        self.core.decide(self.now);
        let scene = self.core.scene().unwrap();
        let meter = scene.windows.iter().flat_map(|w| &w.meters).next().unwrap();
        let MeterState::Live(MeterView::Spectrum {
            overlay: Some(overlay),
            spectrum,
            ..
        }) = &meter.state
        else {
            panic!("no overlay: {:?}", meter.state);
        };
        (*overlay.clone(), spectrum.frequencies.clone())
    }
}

/// The strongest collision and the frequency it's at.
fn strongest(overlay: &SpectrumOverlay, frequencies: &[f32]) -> (f32, f32) {
    overlay
        .collisions
        .iter()
        .zip(frequencies)
        .map(|(&c, &f)| (c, f))
        .fold((0.0, 0.0), |a, b| if b.0 > a.0 { b } else { a })
}

#[test]
fn kick_and_bass_on_one_note_collide_there() {
    let mut app = App::new();
    app.play(55.0, 55.0);
    let (overlay, frequencies) = app.overlay();
    assert_eq!(overlay.name, "Bass");
    assert!(overlay.colour.is_some());
    assert!(!overlay.waiting);
    // On the Spectrum's scale: −12 dBFS at 55 Hz reads about −31 dB with
    // the default 4.5 dB/octave slope, (−31 + 50) / 50 of the way up.
    let (strength, at) = strongest(&overlay, &frequencies);
    assert!(strength > 0.3, "{strength}");
    assert!((at / 55.0).log2().abs() < 0.25, "at {at} Hz");
    // Nowhere near 1 kHz: both are silent there.
    let far = frequencies
        .iter()
        .position(|&f| f > 1_000.0)
        .expect("a point over 1 kHz");
    assert_eq!(overlay.collisions[far], 0.0);
}

#[test]
fn kick_and_bass_apart_dont_collide() {
    let mut app = App::new();
    app.play(50.0, 400.0);
    let (overlay, frequencies) = app.overlay();
    let (strength, at) = strongest(&overlay, &frequencies);
    assert!(strength < 0.2, "{strength} at {at} Hz");
}

#[test]
fn a_gone_overlay_source_is_waited_for() {
    let mut app = App::new();
    app.play(55.0, 55.0);
    let listed = vec![plugin(KICK, "Kick", 0xff_40_20)];
    app.core.handle(Event::SendPlugins(&listed), app.now);
    app.play(55.0, 55.0);
    let (overlay, _) = app.overlay();
    assert!(overlay.waiting);
    assert_eq!(overlay.name, "Bass");
    assert!(overlay.collisions.is_empty());
}
