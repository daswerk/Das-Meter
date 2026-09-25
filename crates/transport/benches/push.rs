//! Microbenchmark for the Send Plugin's audio path: one push of a 64-frame block,
//! as at 48 kHz / 64 samples. Run with `cargo bench -p dasmeter-transport`.
//!
//! It prints the mean, p99 and worst push, and fails if the mean or p99 exceeds
//! 5% of the block's duration (the spec's worst-case `process()` budget). Real
//! pushes take well under a microsecond, so the limit only catches gross regressions.

use std::time::{Duration, Instant};

use dasmeter_transport::{Details, Reader, Writer, remove_table};

const SAMPLE_RATE: f64 = 48_000.0;
const BLOCK: usize = 64;
const PUSHES: usize = 200_000;

fn main() {
    // `cargo test --benches` runs this with `--bench` absent; keep that fast.
    let pushes = if std::env::args().any(|arg| arg == "--bench") {
        PUSHES
    } else {
        1_000
    };

    let name = format!("dmtb{}", std::process::id());
    let details = Details {
        name: "Bench".into(),
        colour: 0,
        mono: false,
        sample_rate: SAMPLE_RATE as u32,
    };
    let claimed = Writer::claim_named(&name, None, &details).expect("claim a slot");
    let mut reader = Reader::open_named(&name).expect("open the table");
    let slot = reader.slots(Instant::now())[0].slot;
    let mut audio = claimed.writer.audio();
    let left = [0.5_f32; BLOCK];
    let right = [-0.5_f32; BLOCK];

    let mut results = Vec::new();
    for listened in [false, true] {
        reader.set_listened(slot, listened);
        for _ in 0..10_000 {
            audio.push(&left, &right); // warm up
        }
        let mut times = Vec::with_capacity(pushes);
        for _ in 0..pushes {
            let start = Instant::now();
            audio.push(std::hint::black_box(&left), std::hint::black_box(&right));
            times.push(start.elapsed());
        }
        times.sort();
        let mean = times.iter().sum::<Duration>() / pushes as u32;
        let p99 = times[pushes * 99 / 100];
        let worst = times[pushes - 1];
        let label = if listened { "listened" } else { "not listened" };
        println!(
            "push {BLOCK} frames ({label}): mean {mean:?}, p99 {p99:?}, worst {worst:?} over {pushes} pushes"
        );
        results.push((label, mean, p99));
    }

    drop(claimed);
    remove_table(&name);

    let budget = Duration::from_secs_f64(BLOCK as f64 / SAMPLE_RATE * 0.05);
    for (label, mean, p99) in results {
        assert!(
            mean <= budget && p99 <= budget,
            "push ({label}) exceeds {budget:?}: mean {mean:?}, p99 {p99:?}"
        );
    }
}
