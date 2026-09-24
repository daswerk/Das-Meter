//! Microbenchmark for the Loudness Meter's analysis (LUFS, LRA, true peak, RMS,
//! peaks) on pink noise at 48 kHz in 512-frame blocks. Run with
//! `cargo bench -p dasmeter-analysis`.
//!
//! It prints the time per block and the share of one core that real-time
//! analysis costs, and fails above 10% of a core: the whole app's budget on the
//! reference Mac, so a regression that big is always a bug.

use std::time::Instant;

use dasmeter_analysis::signals::{both, frames, pink_noise};
use dasmeter_analysis::{LoudnessAnalyser, LoudnessSettings};

const RATE: u32 = 48_000;
const BLOCK: usize = 512;

fn main() {
    let seconds = if std::env::args().any(|arg| arg == "--bench") {
        30.0
    } else {
        1.0
    };
    let audio = both(&pink_noise(frames(RATE, seconds), -6.0, 1));
    let mut analyser = LoudnessAnalyser::new(RATE, LoudnessSettings::default());

    let start = Instant::now();
    for block in audio.chunks(2 * BLOCK) {
        analyser.process(std::hint::black_box(block));
    }
    std::hint::black_box(analyser.readings());
    let elapsed = start.elapsed();

    let blocks = audio.len() / (2 * BLOCK);
    let core_share = elapsed.as_secs_f64() / seconds;
    println!(
        "loudness {BLOCK} frames at {RATE} Hz: {:?} per block, {:.2}% of one core in real time",
        elapsed / blocks as u32,
        core_share * 100.0
    );
    assert!(
        core_share < 0.10,
        "loudness analysis uses {:.2}% of a core",
        core_share * 100.0
    );
}
