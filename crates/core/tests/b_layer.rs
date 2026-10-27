//! A Spectrum's B layer: B's song (A/B) drawn as a grey layer over the mix.

use std::time::Duration;

use dasmeter_analysis::signals::{both, sine};
use dasmeter_core::{
    AppCore, Event, MeterScene, MeterSettings, MeterState, MeterView, SpectrumMeterSettings,
};

const RATE: u32 = 48_000;

fn b_layer(core: &mut AppCore, now: Duration) -> Option<Vec<f32>> {
    core.decide(now);
    let meter: &MeterScene = core
        .scene()?
        .windows
        .iter()
        .flat_map(|w| &w.meters)
        .next()?;
    match &meter.state {
        MeterState::Live(MeterView::Spectrum { b_layer, .. }) => b_layer.clone(),
        other => panic!("not a Spectrum: {other:?}"),
    }
}

#[test]
fn b_shows_as_a_layer_while_it_plays_and_only_when_asked() {
    let shown = SpectrumMeterSettings {
        show_b: true,
        ..SpectrumMeterSettings::default()
    };
    let mut core = AppCore::with_meters(vec![MeterSettings::Spectrum(shown)]);
    let mut now = Duration::ZERO;
    core.handle(Event::StartListening, now);
    core.handle(Event::CaptureStarted { sample_rate: RATE }, now);
    let mix = both(&sine(RATE, 100.0, -20.0, 0.0, RATE as usize));
    let song = both(&sine(RATE, 2_000.0, -12.0, 0.0, RATE as usize));
    for (a, b) in mix.chunks(1_024).zip(song.chunks(1_024)) {
        now += Duration::from_millis(10);
        core.handle(Event::Audio(a), now);
        core.handle(
            Event::BAudio {
                frames: b,
                sample_rate: RATE,
            },
            now,
        );
    }
    let layer = b_layer(&mut core, now).expect("B's layer while it plays");
    assert!(!layer.is_empty());
    let loudest = layer.iter().copied().fold(f32::MIN, f32::max);
    assert!(loudest > -40.0, "B's tone shows: {loudest}");

    // B stops coming: its layer goes.
    for a in mix.chunks(1_024).take(60) {
        now += Duration::from_millis(10);
        core.handle(Event::Audio(a), now);
    }
    assert_eq!(b_layer(&mut core, now), None);

    // Not asked for: no layer.
    core.handle(
        Event::SetMeter {
            meter: 0,
            settings: MeterSettings::Spectrum(SpectrumMeterSettings::default()),
        },
        now,
    );
    for b in song.chunks(1_024).take(20) {
        now += Duration::from_millis(10);
        core.handle(
            Event::BAudio {
                frames: b,
                sample_rate: RATE,
            },
            now,
        );
    }
    assert_eq!(b_layer(&mut core, now), None);
}
