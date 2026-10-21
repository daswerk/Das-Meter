//! The Needle Meter in the app core (#106).

use std::time::Duration;

use dasmeter_analysis::Scale;
use dasmeter_analysis::signals::{both, sine};
use dasmeter_core::{
    AppCore, Event, MeterKind, MeterSettings, MeterState, MeterView, NeedleMeterSettings,
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
            kind: MeterKind::Needle,
        },
        now,
    );
    (core, now)
}

#[test]
fn a_sine_at_the_reference_reads_0_vu_on_two_needles() {
    let (mut core, mut now) = launch();
    let audio = both(&sine(RATE, 1_000.0, -18.0, 0.0, RATE as usize * 2));
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
    let MeterState::Live(MeterView::Needle { needles, .. }) = &meter.state else {
        panic!("not a Needle Meter: {:?}", meter.state);
    };
    assert_eq!(needles.len(), 2);
    for needle in needles {
        let db = needle.level.db().expect("a level");
        assert!((db - -18.0).abs() < 0.3, "{db}");
        assert!(!needle.over);
    }
}

#[test]
fn scales_without_ballistics_fall_back_to_vu() {
    let (mut core, now) = launch();
    core.handle(
        Event::SetMeter {
            meter: 0,
            settings: MeterSettings::Needle(NeedleMeterSettings {
                scale: Scale::K14,
                ..NeedleMeterSettings::default()
            }),
        },
        now,
    );
    let Some(MeterSettings::Needle(settings)) = core.meter_settings(0) else {
        panic!("not a Needle Meter");
    };
    assert_eq!(settings.scale, Scale::Vu);
}
