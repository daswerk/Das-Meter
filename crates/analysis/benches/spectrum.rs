//! Microbenchmark for the Spectrum's FFT path at the default size (8192, Hann,
//! 512-point line, stereo L+R). Run with `cargo bench -p dasmeter-analysis`.
//!
//! One update per drawn frame at 60 fps. It prints the time per update and the
//! share of one core at 60 updates a second, and fails above 10% of a core.

use std::time::{Duration, Instant};

use dasmeter_analysis::signals::{frames, pink_noise, stereo};
use dasmeter_analysis::{ChannelView, SpectrumAnalyser, SpectrumSettings};

const RATE: u32 = 48_000;
const FPS: usize = 60;

fn main() {
    let updates = if std::env::args().any(|arg| arg == "--bench") {
        3_000
    } else {
        30
    };
    let block = frames(RATE, 1.0 / FPS as f64);
    let noise = pink_noise(block * 64, -6.0, 3);
    let audio = stereo(&noise, &noise[block..]);
    let settings = SpectrumSettings {
        channel_view: ChannelView::LeftRight,
        ..Default::default()
    };
    let mut analyser = SpectrumAnalyser::new(RATE, settings);

    let mut total = Duration::ZERO;
    let chunks: Vec<&[f32]> = audio.chunks_exact(2 * block).collect();
    for i in 0..updates {
        analyser.process(chunks[i % chunks.len()]);
        let start = Instant::now();
        std::hint::black_box(analyser.update());
        total += start.elapsed();
    }

    let per_update = total / updates as u32;
    let core_share = per_update.as_secs_f64() * FPS as f64;
    println!(
        "spectrum FFT 8192, L+R: {per_update:?} per update, {:.2}% of one core at {FPS} fps",
        core_share * 100.0
    );
    assert!(
        core_share < 0.10,
        "spectrum uses {:.2}% of a core",
        core_share * 100.0
    );
}
