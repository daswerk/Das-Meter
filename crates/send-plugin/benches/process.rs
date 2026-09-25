//! Microbenchmark for the Send Plugin's `process()` through an in-process host,
//! at 48 kHz / 64 samples. Run with `cargo bench -p dasmeter-send`.
//!
//! Budget (spec): on average ≤ 0.1% of a core, and no call over 5% of the
//! block's duration. It prints the mean, p99.9 and worst call, and fails if the
//! mean or p99.9 is over budget. The worst single call is printed but not
//! asserted: on a shared CI runner the OS can preempt any one call.

#[path = "../tests/common/mod.rs"]
mod common;

use std::time::{Duration, Instant};

use common::Instance;
use dasmeter_transport::{Reader, remove_table};

const SAMPLE_RATE: f64 = 48_000.0;
const BLOCK: usize = 64;
const CALLS: usize = 200_000;

fn main() {
    // `cargo test --benches` runs this without `--bench`; keep that fast.
    let calls = if std::env::args().any(|arg| arg == "--bench") {
        CALLS
    } else {
        1_000
    };

    let name = format!("dmspb{}", std::process::id());
    let mut plugin = Instance::new(&name, None, BLOCK as u32);
    let mut reader = Reader::open_named(&name).expect("open the table");
    let slot = reader.slots(Instant::now())[0].slot;
    let mut left = [0.5_f32; BLOCK];
    let mut right = [-0.5_f32; BLOCK];
    let mut out = [vec![0.0; BLOCK], vec![0.0; BLOCK]];

    let block = Duration::from_secs_f64(BLOCK as f64 / SAMPLE_RATE);
    let mean_budget = block / 1_000; // 0.1% of a core
    let call_budget = block / 20; // 5% of the block

    let mut failures = Vec::new();
    for listened in [false, true] {
        reader.set_listened(slot, listened);
        for _ in 0..10_000 {
            plugin.process(&mut left, &mut right, &mut out); // warm up
        }
        let mut times = Vec::with_capacity(calls);
        for _ in 0..calls {
            let start = Instant::now();
            plugin.process(&mut left, &mut right, &mut out);
            times.push(start.elapsed());
        }
        times.sort();
        let mean = times.iter().sum::<Duration>() / calls as u32;
        let p999 = times[calls * 999 / 1000];
        let worst = times[calls - 1];
        let label = if listened { "listened" } else { "not listened" };
        println!(
            "process {BLOCK} frames ({label}): mean {mean:?} ({:.4}% of a core), p99.9 {p999:?}, worst {worst:?} over {calls} calls",
            mean.as_secs_f64() / block.as_secs_f64() * 100.0
        );
        if mean > mean_budget {
            failures.push(format!("{label}: mean {mean:?} > {mean_budget:?}"));
        }
        if p999 > call_budget {
            failures.push(format!("{label}: p99.9 {p999:?} > {call_budget:?}"));
        }
    }

    drop(plugin);
    remove_table(&name);
    assert!(failures.is_empty(), "over budget: {failures:?}");
}
