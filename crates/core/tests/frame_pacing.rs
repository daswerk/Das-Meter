//! New frames keep to the display's rhythm: counted from the refresh each
//! one lands on, so one late wake-up doesn't push the frames after it.

use std::time::Duration;

use dasmeter_analysis::signals::{both, sine};
use dasmeter_core::{AppCore, AppSettings, Decision, Event};

const RATE: u32 = 48_000;
const REFRESH: u32 = 120;

/// The refreshes (numbered) that new frames landed on, the shell drawing
/// each as the core asks and its frame ready on the next refresh. The wake
/// for the frame after `late_after` comes 3 ms late.
fn refreshes(late_after: usize) -> Vec<u64> {
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
    core.handle(Event::DisplayRefreshRate(REFRESH), now);
    core.handle(
        Event::SetApp(AppSettings {
            match_display: true,
            frame_rate_cap: 60,
            ..AppSettings::default()
        }),
        now,
    );
    let audio = both(&sine(RATE, 440.0, -12.0, 0.0, RATE as usize * 3));
    let period = 1.0 / f64::from(REFRESH);
    let block = 512;
    let mut fed = 0;
    let mut landed = Vec::new();
    while landed.len() < 120 && fed + block * 2 <= audio.len() {
        // Audio as it arrives, up to now.
        while (fed / 2) as f64 / f64::from(RATE) <= now.as_secs_f64() {
            core.handle(Event::Audio(&audio[fed..fed + block * 2]), now);
            fed += block * 2;
        }
        match core.decide(now) {
            Decision::Draw => {
                // The frame is ready on the next refresh.
                let refresh = (now.as_secs_f64() / period).ceil() as u64;
                core.retime(Duration::from_secs_f64(refresh as f64 * period));
                landed.push(refresh);
                now = Duration::from_secs_f64(refresh as f64 * period + 0.0001);
            }
            Decision::Sleep { until } => {
                let late = if landed.len() == late_after + 1 {
                    Duration::from_millis(3)
                } else {
                    Duration::ZERO
                };
                let next = until.unwrap_or(now + Duration::from_millis(5));
                now = next.max(now + Duration::from_micros(100)) + late;
            }
        }
    }
    landed
}

#[test]
fn a_late_wake_up_doesnt_push_the_frames_after_it() {
    let landed = refreshes(40);
    let gaps: Vec<u64> = landed.windows(2).skip(10).map(|w| w[1] - w[0]).collect();
    let off = gaps.iter().filter(|&&g| g != 2).count();
    // At most the late one lands a refresh later, and the next on time again.
    assert!(off <= 2, "gaps between new frames, in refreshes: {gaps:?}");
}
