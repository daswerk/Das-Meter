//! Microbenchmarks for the Stereometer and Waveform analysers on pink noise at
//! 48 kHz in 512-frame blocks. Run with `cargo bench -p dasmeter-analysis`.
//!
//! Each prints the share of one core that real-time analysis costs and fails
//! above 10% of a core.

use std::time::Instant;

use dasmeter_analysis::signals::{frames, pink_noise, stereo};
use dasmeter_analysis::{
    ChannelView, StereometerAnalyser, StereometerSettings, WaveformAnalyser, WaveformSettings,
};

const RATE: u32 = 48_000;
const BLOCK: usize = 512;

fn measure(name: &str, seconds: f64, audio: &[f32], mut process: impl FnMut(&[f32])) {
    let start = Instant::now();
    for block in audio.chunks(2 * BLOCK) {
        process(std::hint::black_box(block));
    }
    let core_share = start.elapsed().as_secs_f64() / seconds;
    println!(
        "{name} {BLOCK} frames at {RATE} Hz: {:.2}% of one core in real time",
        core_share * 100.0
    );
    assert!(
        core_share < 0.10,
        "{name} uses {:.2}% of a core",
        core_share * 100.0
    );
}

fn main() {
    let seconds = if std::env::args().any(|arg| arg == "--bench") {
        30.0
    } else {
        1.0
    };
    let n = frames(RATE, seconds);
    let audio = stereo(&pink_noise(n, -6.0, 5), &pink_noise(n, -6.0, 6));

    let mut stereometer = StereometerAnalyser::new(RATE, StereometerSettings::default());
    let mut points = Vec::new();
    measure("stereometer", seconds, &audio, |block| {
        stereometer.process(block);
        std::hint::black_box(stereometer.readings());
    });
    stereometer.points(Default::default(), &mut points);
    std::hint::black_box(&points);

    let settings = WaveformSettings {
        channel_view: ChannelView::LeftRight,
        ..Default::default()
    };
    let mut waveform = WaveformAnalyser::new(RATE, settings);
    measure("waveform (L/R)", seconds, &audio, |block| {
        waveform.process(block)
    });
    std::hint::black_box(waveform.columns(0).count());
}
