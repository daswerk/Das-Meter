//! The Tuner Meter in the app core (#103): its reading and its settings' ranges.

use std::f64::consts::TAU;
use std::time::Duration;

use dasmeter_core::{
    AppCore, Event, MeterKind, MeterSettings, MeterState, MeterView, TunerMeterSettings,
};

const RATE: u32 = 48_000;

fn launch() -> (AppCore, Duration) {
    let mut core = AppCore::new();
    let now = Duration::ZERO;
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
            kind: MeterKind::Tuner,
        },
        now,
    );
    (core, now)
}

#[test]
fn a_tone_reads_as_its_note_and_cents() {
    let (mut core, mut now) = launch();
    let hz = 110.0 * 2f64.powf(-12.0 / 1_200.0);
    let audio: Vec<f32> = (0..RATE as usize * 2)
        .flat_map(|i| {
            let t = i as f64 / f64::from(RATE);
            let x: f64 = (1..=10)
                .map(|k| 0.2 / f64::from(k) * (TAU * hz * f64::from(k) * t).sin())
                .sum();
            [x as f32, x as f32]
        })
        .collect();
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
    let MeterState::Live(MeterView::Tuner {
        reading: Some(reading),
        ..
    }) = &meter.state
    else {
        panic!("no reading: {:?}", meter.state);
    };
    assert_eq!((reading.note.name, reading.note.octave), ("A", 2));
    assert!(
        (reading.note.cents + 12.0).abs() <= 2.0,
        "{}",
        reading.note.cents
    );
    assert!(!reading.held);
}

#[test]
fn the_reference_pitch_stays_in_range() {
    let (mut core, now) = launch();
    core.handle(
        Event::SetMeter {
            meter: 0,
            settings: MeterSettings::Tuner(TunerMeterSettings {
                a4: 500.0,
                ..TunerMeterSettings::default()
            }),
        },
        now,
    );
    let Some(MeterSettings::Tuner(settings)) = core.meter_settings(0) else {
        panic!("not a Tuner");
    };
    assert_eq!(settings.a4, 466.0);
}
