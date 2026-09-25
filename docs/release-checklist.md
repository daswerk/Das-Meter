# Release checklist

Steps a release needs besides a green CI. Each blocks the release if it fails.

## Performance budget

Run on both reference machines, each on its built-in display, on mains power,
with nothing else busy:

- a base **M1 MacBook Air** (8 GB);
- an **i5-1135G7 / Ryzen 5 5500U-class** laptop with integrated graphics.

```sh
cargo build --release -p dasmeter-app
./target/release/das-meter --benchmark both
```

It runs the Busy and Silent scenes three times each (5 s warm-up, 60 s
measured) and prints the best run of each. While it runs, keep Activity
Monitor ▸ Window ▸ GPU History open for the GPU figure, which the benchmark
can't read itself.

Compare with the budget ([Set the performance budget](https://github.com/daswerk/Das-Meter/issues/10)):

| Busy scene                | M1 MacBook Air | Windows iGPU laptop |
|---------------------------|----------------|---------------------|
| App CPU (% of one core)   | ≤ 10 %         | ≤ 15 %              |
| GPU                       | ≤ 10 %         | ≤ 15 %              |
| Resident memory           | ≤ 150 MB       | ≤ 150 MB            |
| Late frames (> 16.7 ms)   | ≤ 1 %, no stall over 50 ms after warm-up | same |

- **Silent scene**: CPU ≤ 1 % of a core, and no frames drawn.
- **Latency** (click → screen, from the test signal's clicks): about 2 frames (~35 ms).

A miss blocks the release: note the numbers in the release issue and fix, or
reopen the budget issue with the measurements.

## Loudness conformance

Run the EBU test set at 48 kHz (`--features ebu-conformance`), per
[ADR 0005](adr/0005-measurement-definitions-and-verification.md). The EBU
files are downloaded by hand and never committed.
