# Measurements

Das-Meter claims its **numbers**, not a display standard. The Loudness Meter
measures to **ITU-R BS.1770-5** and **EBU Tech 3341/3342** (momentary,
short-term and integrated LUFS, LRA, true peak), checked against the EBU test
set. It does not claim "EBU Mode", because that also requires the EBU +9/+18
display scales, which Das-Meter doesn't have yet.

These definitions are fixed, because you compare readings across meters, and
changing them later would silently move everyone's numbers.

## Definitions

- **LUFS, LRA, true peak:** the `ebur128` crate, pinned to an exact version.
  True peak oversamples 4× below 96 kHz and 2× at 96 kHz.
- **Sample peak:** `20·log10(max|x|)` per channel.
- **Counters:** per channel, since the last reset. **Clips**: runs of samples
  with `|x| ≥ 1.0` (0 dBFS), each run counted once. **TP over**: runs of true
  peak above the ceiling (−1 dBTP by default), looked at in 10 ms blocks, so
  overs less than 10 ms apart count once.
- **RMS:** AES17 by default (+3.01 dB, so a 0 dBFS sine reads 0 dB), labelled
  "RMS (AES17)"; plain RMS is an option. A sliding rectangular window, 300 ms by
  default.
- **Correlation:** Pearson `Σ(L·R)/√(Σ(L²)·Σ(R²))`, no DC removal, the sums
  averaged exponentially with τ = 300 ms. With both channels below −90 dBFS it
  reads 0 and dims as "no signal"; with only one channel carrying signal it
  reads 0.
- **Cycle correlation** (the Phase Scope's number, not a measurement this page
  claims): Pearson between the main and Overlay Source's lows, below the
  **Cycle correlation below** frequency, about each one's mean, over the last
  whole Cycle. It answers "do kick and bass fit?", so it's named apart from
  the Correlation above.
- **Spectrum:** sine-calibrated (a 0 dBFS sine on a bin reads 0 dB, corrected
  for the window's coherent gain), the slope pivoted at 1 kHz, a Hann window by
  default.

## How it's checked

- **Every change:** automated tests on signals we generate ourselves at 44.1,
  48, 96 and 192 kHz. Tolerances: LUFS ±0.1 LU, LRA ±1 LU, true peak
  +0.2/−0.4 dB, sample peak ±0.01 dB, RMS ±0.05 dB, correlation ±0.02; a
  Spectrum sine on a bin ±0.1 dB.
- **Every release:** the EBU conformance test set at 48 kHz.

There's no official certification, so there's no badge.

## Known limits

- **192 kHz:** true peak isn't oversampled there, so it's only a sample peak.
  The Loudness Meter then labels it **Peak** instead of **TP**, so it never
  claims more than it measures.
- **System Capture** measures what the system mixer plays. For levels
  straight from a DAW track, use a [Send Plugin](send-plugins.md).
- A Spectrum sine half a bin off-centre reads up to 1.42 dB low (the Hann
  window's scalloping loss).
