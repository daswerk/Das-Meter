# Measurement definitions and how they are verified

Das-Meter claims its **numbers**, not a display standard: the Loudness Meter measures to ITU-R BS.1770-5 and EBU Tech 3341/3342 (momentary, short-term and integrated LUFS, LRA, true peak), checked against the EBU test set. It does not claim "EBU Mode", because that also requires the EBU +9/+18 display scales, which v1 doesn't have. The definitions below are fixed because users compare readings across meters, and changing them later silently moves everyone's numbers.

## Definitions

- **LUFS, LRA, true peak**: the `ebur128` crate, pinned to an exact version so expected values only move when we choose to update. Its true peak oversamples 4× below 96 kHz, 2× at 96 kHz and not at all at 192 kHz; at 192 kHz the Loudness Meter's label changes from "TP" to "Peak" so it never claims more than it measures.
- **Sample peak**: `20·log10(max|x|)` per channel.
- **RMS**: AES17 by default (+3.01 dB, so a 0 dBFS sine reads 0 dB), labelled "RMS (AES17)"; plain RMS is an advanced option. Sliding rectangular 300 ms window, so a steady sine gives an exact, testable value.
- **Correlation**: Pearson `Σ(L·R)/√(Σ(L²)·Σ(R²))`, no DC removal, sums averaged exponentially with τ = 300 ms. Both channels below −90 dBFS: reads 0 and dims as "no signal". Only one channel with signal: reads 0.
- **Spectrum**: sine-calibrated (a 0 dBFS sine on a bin reads 0 dB, coherent-gain corrected), slope pivoted at 1 kHz, Hann window by default.

## Verification

- **Every pull request (CI, blocks merge)**: `cargo test` golden tests on synthetic signals we generate ourselves at 44.1, 48, 96 and 192 kHz. Tolerances: LUFS ±0.1 LU, LRA ±1 LU, true peak +0.2/−0.4 dB, sample peak ±0.01 dB, RMS ±0.05 dB, correlation ±0.02; Spectrum sine on a bin ±0.1 dB, half a bin off within Hann's 1.42 dB scalloping loss, peak frequency within half a bin. Kept separate from the performance microbenchmarks: correctness blocks merges, performance blocks releases.
- **EBU conformance (48 kHz, blocks a release)**: an opt-in `--features ebu-conformance` test run on the dev machine against a hand-downloaded EBU test set, one step on the release checklist. The EBU files may not be redistributed, so they are **never committed**. CI may try the download with a cache; a failed download counts as "skipped", never "passed".

## Where the claim appears

The docs site's Measurements page (`docs/site/src/measurements.md`: standards, definitions, known limits), one line in the README, and a one-line note with a link in the Loudness Meter's settings panel. No badge: there is no official certification.
