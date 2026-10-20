//! The Stereo Bands Meter in the app core (#100).

use std::time::Duration;

use dasmeter_analysis::signals::{both, sine};
use dasmeter_core::{AppCore, Event, MeterKind, MeterState, MeterView};

const RATE: u32 = 48_000;

#[test]
fn a_mono_tone_reads_plus_one_in_its_band_and_nothing_where_its_silent() {
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
            kind: MeterKind::StereoBands,
        },
        now,
    );
    let audio = both(&sine(RATE, 1_000.0, -12.0, 0.0, RATE as usize));
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
    let MeterState::Live(MeterView::StereoBands { bands, .. }) = &meter.state else {
        panic!("not Stereo Bands: {:?}", meter.state);
    };
    assert_eq!(bands.len(), 10);
    let at = |centre: f32| bands.iter().find(|b| b.centre == centre).unwrap();
    assert_eq!(at(1_000.0).correlation, Some(1.0));
    assert_eq!(at(1_000.0).width, Some(0.0));
    assert_eq!(at(31.25).correlation, None);
}
