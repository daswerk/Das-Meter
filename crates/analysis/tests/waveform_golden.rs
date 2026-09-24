//! Tests for the Waveform's analysis at every supported sample rate.

use std::time::Duration;

use dasmeter_analysis::signals::{amplitude, both, frames, silence, sine, stereo};
use dasmeter_analysis::{ChannelView, WaveformAnalyser, WaveformScale, WaveformSettings};

const RATES: [u32; 4] = [44_100, 48_000, 96_000, 192_000];

fn analyse(rate: u32, settings: WaveformSettings, audio: &[f32]) -> WaveformAnalyser {
    let mut analyser = WaveformAnalyser::new(rate, settings);
    for block in audio.chunks(1024) {
        analyser.process(block);
    }
    analyser
}

/// Mean band RMS over the second half of the columns (after the filters settle).
fn bands(analyser: &WaveformAnalyser, trace: usize) -> [f32; 3] {
    let columns: Vec<_> = analyser.columns(trace).collect();
    let settled = &columns[columns.len() / 2..];
    let mean = |f: fn(&&dasmeter_analysis::WaveformColumn) -> f32| {
        settled.iter().map(f).sum::<f32>() / settled.len() as f32
    };
    [mean(|c| c.low), mean(|c| c.mid), mean(|c| c.high)]
}

fn db(x: f32) -> f32 {
    20.0 * x.log10()
}

#[test]
fn a_low_sine_lands_in_the_low_band_and_a_high_sine_in_the_high_band() {
    for rate in RATES {
        let n = frames(rate, 1.0);
        let sine_rms = amplitude(-6.0) as f32 / 2f32.sqrt();
        // A Linkwitz-Riley 4th-order crossover is −0.53 dB an octave inside its
        // band (100 Hz against 200 Hz), so allow 1 dB.

        let low = analyse(
            rate,
            WaveformSettings::default(),
            &both(&sine(rate, 100.0, -6.0, 0.0, n)),
        );
        let [l, m, h] = bands(&low, 0);
        assert!(
            (db(l) - db(sine_rms)).abs() < 1.0,
            "100 Hz low band {:.2} dB at {rate} Hz",
            db(l)
        );
        assert!(
            db(m) < db(l) - 12.0 && db(h) < db(l) - 40.0,
            "100 Hz leaks: mid {:.1}, high {:.1}",
            db(m),
            db(h)
        );

        let high = analyse(
            rate,
            WaveformSettings::default(),
            &both(&sine(rate, 5_000.0, -6.0, 0.0, n)),
        );
        let [l, m, h] = bands(&high, 0);
        assert!(
            (db(h) - db(sine_rms)).abs() < 1.0,
            "5 kHz high band {:.2} dB at {rate} Hz",
            db(h)
        );
        assert!(
            db(m) < db(h) - 12.0 && db(l) < db(h) - 40.0,
            "5 kHz leaks: low {:.1}, mid {:.1}",
            db(l),
            db(m)
        );

        let mid = analyse(
            rate,
            WaveformSettings::default(),
            &both(&sine(rate, 630.0, -6.0, 0.0, n)),
        );
        let [l, m, h] = bands(&mid, 0);
        assert!(m > l && m > h, "630 Hz lands in mid at {rate} Hz");
    }
}

#[test]
fn crossovers_are_configurable() {
    let rate = 48_000;
    let settings = WaveformSettings {
        low_crossover: 50.0,
        high_crossover: 500.0,
        ..Default::default()
    };
    let n = frames(rate, 1.0);
    let analyser = analyse(rate, settings, &both(&sine(rate, 100.0, -6.0, 0.0, n)));
    let [l, m, h] = bands(&analyser, 0);
    assert!(
        m > l && m > h,
        "100 Hz is mid with crossovers at 50 Hz / 500 Hz"
    );
}

#[test]
fn the_envelope_peak_matches_the_sine_amplitude() {
    for rate in RATES {
        let n = frames(rate, 1.0);
        let analyser = analyse(
            rate,
            WaveformSettings::default(),
            &both(&sine(rate, 1_000.0, -6.0, 0.0, n)),
        );
        let (low, high) = analyser.columns(0).fold((0.0f32, 0.0f32), |(lo, hi), c| {
            (lo.min(c.min), hi.max(c.max))
        });
        let want = amplitude(-6.0) as f32;
        assert!(
            (high - want).abs() < 0.001 && (low + want).abs() < 0.001,
            "envelope {low}..{high} at {rate} Hz"
        );
    }
}

#[test]
fn the_time_span_sets_the_column_count() {
    for rate in RATES {
        for (seconds, columns) in [(1, 200), (4, 800), (30, 6_000)] {
            let settings = WaveformSettings {
                span: Duration::from_secs(seconds),
                ..Default::default()
            };
            assert_eq!(settings.columns(), columns);
            // Feed more than the span: it scrolls and keeps exactly that many.
            let audio = both(&silence(frames(rate, seconds as f64 + 0.5)));
            let analyser = analyse(rate, settings, &audio);
            assert_eq!(
                analyser.columns(0).len(),
                columns,
                "{seconds} s at {rate} Hz"
            );
        }
    }
}

#[test]
fn a_shorter_span_keeps_the_newest_columns() {
    let rate = 48_000;
    let mut analyser = analyse(
        rate,
        WaveformSettings::default(),
        &both(&silence(frames(rate, 3.0))),
    );
    analyser.process(&both(&sine(rate, 1_000.0, 0.0, 0.0, frames(rate, 1.0))));
    analyser.set_settings(WaveformSettings {
        span: Duration::from_secs(1),
        ..Default::default()
    });
    assert_eq!(analyser.columns(0).len(), 200);
    assert!(
        analyser.columns(0).all(|c| c.max > 0.9),
        "only the newest second is kept"
    );
}

#[test]
fn channel_views_split_the_traces() {
    let rate = 48_000;
    let n = frames(rate, 0.5);
    let audio = stereo(&sine(rate, 1_000.0, 0.0, 0.0, n), &silence(n));
    let peak = |a: &WaveformAnalyser, t| a.columns(t).fold(0.0f32, |m, c| m.max(c.max));

    let mono = analyse(rate, WaveformSettings::default(), &audio);
    assert_eq!(mono.traces(), 1);
    assert!(
        (peak(&mono, 0) - 0.5).abs() < 0.001,
        "mono sum halves a one-sided signal"
    );

    let lr = analyse(
        rate,
        WaveformSettings {
            channel_view: ChannelView::LeftRight,
            ..Default::default()
        },
        &audio,
    );
    assert_eq!(lr.traces(), 2);
    assert!((peak(&lr, 0) - 1.0).abs() < 0.001);
    assert_eq!(peak(&lr, 1), 0.0);

    let ms = analyse(
        rate,
        WaveformSettings {
            channel_view: ChannelView::MidSide,
            ..Default::default()
        },
        &both(&sine(rate, 1_000.0, 0.0, 0.0, n)),
    );
    assert_eq!(peak(&ms, 1), 0.0, "side of a mono signal is silent");
}

#[test]
fn heights_follow_scale_and_gain() {
    let linear = WaveformSettings::default();
    assert_eq!(linear.height(0.5), 0.5);
    assert_eq!(linear.height(-0.5), -0.5);
    let zoomed = WaveformSettings {
        gain: 4.0,
        ..linear
    };
    assert_eq!(zoomed.height(0.5), 1.0, "clamped at the edge");

    let db = WaveformSettings {
        scale: WaveformScale::Decibels,
        ..linear
    };
    assert!((db.height(1.0) - 1.0).abs() < 1e-6);
    assert!((db.height(amplitude(-30.0) as f32) - 0.5).abs() < 1e-4);
    assert!((db.height(-(amplitude(-30.0) as f32)) + 0.5).abs() < 1e-4);
    assert_eq!(db.height(amplitude(-80.0) as f32), 0.0);
    assert_eq!(db.height(0.0), 0.0);
}
