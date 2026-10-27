//! The Goniometer in the app core: L against R, mono upright.

use std::time::Duration;

use dasmeter_analysis::signals::{both, sine};
use dasmeter_core::{AppCore, Event, MeterKind, MeterState, MeterView};

const RATE: u32 = 48_000;

/// The Goniometer's view after `audio` (interleaved stereo) plays.
fn goniometer_after(audio: &[f32]) -> (Vec<[f32; 2]>, f32, f32) {
    let mut core = AppCore::new();
    let mut now = Duration::ZERO;
    core.handle(
        Event::PresetFiles {
            files: &[],
            settings: None,
        },
        now,
    );
    core.handle(Event::StartListening, now);
    core.handle(Event::CaptureStarted { sample_rate: RATE }, now);
    core.handle(
        Event::AssignMeter {
            meter: 0,
            kind: MeterKind::Goniometer,
        },
        now,
    );
    for block in audio.chunks(2048) {
        now += Duration::from_secs_f64(1024.0 / f64::from(RATE));
        core.handle(Event::Audio(block), now);
        core.decide(now);
    }
    now += Duration::from_millis(40);
    core.decide(now);
    let scene = core.scene().unwrap();
    let meter = scene
        .windows
        .iter()
        .flat_map(|w| &w.meters)
        .find(|m| m.meter == 0)
        .unwrap();
    let MeterState::Live(MeterView::Goniometer {
        points,
        readings,
        scale_db,
        ..
    }) = &meter.state
    else {
        panic!("not a Goniometer: {:?}", meter.state);
    };
    (points.clone(), readings.correlation, *scale_db)
}

#[test]
fn a_mono_tone_stands_upright() {
    let (points, correlation, _) =
        goniometer_after(&both(&sine(RATE, 440.0, -12.0, 0.0, RATE as usize)));
    // 120 ms of points at 48 kHz.
    assert_eq!(points.len(), 5_760);
    assert!(
        points.iter().all(|[x, _]| x.abs() < 1e-3),
        "mono has no side"
    );
    assert!(
        points.iter().any(|[_, y]| y.abs() > 0.5),
        "and fills the scope"
    );
    assert_eq!(correlation, 1.0);
}

#[test]
fn out_of_phase_lies_flat() {
    let tone = sine(RATE, 440.0, -12.0, 0.0, RATE as usize);
    let audio: Vec<f32> = tone.iter().flat_map(|&s| [s, -s]).collect();
    let (points, correlation, _) = goniometer_after(&audio);
    assert!(points.iter().all(|[_, y]| y.abs() < 1e-3), "no mid at all");
    assert_eq!(correlation, -1.0);
}

#[test]
fn a_quiet_signal_is_scaled_up_and_says_by_how_much() {
    let (_, _, loud) = goniometer_after(&both(&sine(RATE, 440.0, -6.0, 0.0, RATE as usize)));
    let (_, _, quiet) = goniometer_after(&both(&sine(RATE, 440.0, -30.0, 0.0, RATE as usize)));
    assert!(quiet > loud + 12.0, "quiet {quiet} dB, loud {loud} dB");
    assert_eq!(quiet * 2.0, (quiet * 2.0).round(), "to 0.5 dB");
}
