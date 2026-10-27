//! Golden tests for the Oscilloscope: a periodic wave stands still on its
//! rising edge, the window holds as many periods as it should, and silence
//! gives a flat, unchanging picture.

use std::time::Duration;

use dasmeter_analysis::oscilloscope::POINTS;
use dasmeter_analysis::signals::{both, frames, silence, stereo};
use dasmeter_analysis::{ChannelView, OscilloscopeAnalyser, OscilloscopeSettings};

const RATE: u32 = 48_000;

fn sine(frequency: f64, amplitude: f64, seconds: f64) -> Vec<f32> {
    (0..frames(RATE, seconds))
        .map(|i| {
            let t = i as f64 / f64::from(RATE);
            (amplitude * (std::f64::consts::TAU * frequency * t).sin()) as f32
        })
        .collect()
}

fn settings(window_ms: u64) -> OscilloscopeSettings {
    OscilloscopeSettings {
        window: Duration::from_millis(window_ms),
        auto_gain: false,
        ..OscilloscopeSettings::default()
    }
}

/// Feeds `audio` (interleaved stereo) in blocks of `block` frames.
fn analyse(settings: OscilloscopeSettings, audio: &[f32], block: usize) -> OscilloscopeAnalyser {
    let mut analyser = OscilloscopeAnalyser::new(RATE, settings);
    for chunk in audio.chunks(2 * block) {
        analyser.process(chunk);
    }
    analyser
}

/// Rising zero crossings in a trace.
fn rising(trace: &[f32]) -> usize {
    trace
        .windows(2)
        .filter(|p| p[0] < 0.0 && p[1] >= 0.0)
        .count()
}

#[test]
fn a_sine_starts_on_its_rising_edge() {
    let audio = both(&sine(441.0, 0.5, 0.5));
    let mut analyser = analyse(settings(10), &audio, 512);
    let scope = analyser.update();
    assert!(scope.triggered);
    let trace = &scope.traces[0];
    assert_eq!(trace.len(), POINTS);
    assert!(trace[0].abs() < 0.02, "starts at {}", trace[0]);
    assert!(
        trace[4] > trace[0],
        "rising: {} then {}",
        trace[0],
        trace[4]
    );
}

#[test]
fn the_picture_stands_still_whatever_the_block_size() {
    // Different amounts of audio, cut at different places in the cycle.
    let a = both(&sine(441.0, 0.5, 0.5));
    let b = both(&sine(441.0, 0.5, 0.537));
    let first = analyse(settings(10), &a, 512).update().traces[0].clone();
    let second = analyse(settings(10), &b, 333).update().traces[0].clone();
    let worst = first
        .iter()
        .zip(&second)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f32::max);
    assert!(worst < 0.02, "differs by up to {worst}");
}

#[test]
fn the_window_holds_its_span_of_time() {
    // 1 kHz over 10 ms: ten periods.
    let audio = both(&sine(1_000.0, 0.5, 0.5));
    let scope = analyse(settings(10), &audio, 512).update().clone();
    let periods = rising(&scope.traces[0]);
    assert!((9..=11).contains(&periods), "{periods} periods");
    assert_eq!(scope.window, Duration::from_millis(10));
}

#[test]
fn free_running_shows_the_newest_audio() {
    let audio = both(&sine(441.0, 0.5, 0.5));
    let mut analyser = analyse(
        OscilloscopeSettings {
            trigger: false,
            ..settings(10)
        },
        &audio,
        512,
    );
    let scope = analyser.update();
    assert!(!scope.triggered);
    // It ends on the last sample fed.
    let last = *audio.last().unwrap();
    let shown = *scope.traces[0].last().unwrap();
    assert!((shown - last).abs() < 0.02, "{shown} vs {last}");
}

#[test]
fn left_right_shows_both_channels() {
    let left = sine(441.0, 0.5, 0.5);
    let right: Vec<f32> = left.iter().map(|x| -0.5 * x).collect();
    let audio = stereo(&left, &right);
    let scope = analyse(
        OscilloscopeSettings {
            channels: ChannelView::LeftRight,
            ..settings(10)
        },
        &audio,
        512,
    )
    .update()
    .clone();
    assert_eq!(scope.traces.len(), 2);
    for (l, r) in scope.traces[0].iter().zip(&scope.traces[1]) {
        assert!((r + 0.5 * l).abs() < 0.01, "{l} {r}");
    }
}

#[test]
fn auto_gain_brings_a_quiet_wave_up() {
    let audio = both(&sine(441.0, 0.01, 1.0));
    let mut analyser = analyse(
        OscilloscopeSettings {
            auto_gain: true,
            ..settings(10)
        },
        &audio,
        512,
    );
    let scope = analyser.update().clone();
    let peak = scope.traces[0].iter().fold(0.0f32, |m, x| m.max(x.abs()));
    assert!(peak > 0.5 && peak <= 1.0, "peak {peak}");
    assert!(scope.gain_db > 30.0, "{} dB", scope.gain_db);

    // Off: as it is.
    let scope = analyse(settings(10), &audio, 512).update().clone();
    let peak = scope.traces[0].iter().fold(0.0f32, |m, x| m.max(x.abs()));
    assert!((peak - 0.01).abs() < 0.002, "peak {peak}");
    assert_eq!(scope.gain_db, 0.0);
}

#[test]
fn silence_is_flat_and_stays_the_same() {
    let audio = both(&silence(frames(RATE, 0.5)));
    let mut analyser = analyse(
        OscilloscopeSettings {
            auto_gain: true,
            ..settings(10)
        },
        &audio,
        512,
    );
    let first = analyser.update().clone();
    assert!(first.traces[0].iter().all(|&x| x == 0.0));
    analyser.process(&both(&silence(frames(RATE, 0.1))));
    assert_eq!(*analyser.update(), first);
}
