# Measurements

Das-Meter claims its **numbers**, not a display standard. The Loudness Meter
measures to **ITU-R BS.1770-5** and **EBU Tech 3341/3342** (momentary,
short-term and integrated LUFS, LRA, true peak), tested on those standards'
cases (see [How it's checked](#how-its-checked)). It does not claim "EBU Mode", because that also requires the EBU +9/+18
display scales, which Das-Meter doesn't have yet.

These definitions are fixed, because you compare readings across meters, and
changing them later would silently move everyone's numbers.

## Definitions

- **LUFS, LRA, true peak:** the `ebur128` crate, pinned to an exact version.
  True peak oversamples 4× below 96 kHz and 2× at 96 kHz.
- **Sample peak:** `20·log10(max|x|)` per channel.
- **Scales** (the bars' VU, PPM and K-System ballistics): defined in
  [ADR 0007](https://github.com/daswerk/Das-Meter/blob/main/docs/adr/0007-meter-scales-and-ballistics.md).
  VU reaches 99% in 300 ms with 1.25% overshoot; PPM Type I (DIN, Nordic)
  reads 80% of a 5 ms burst and falls 20 dB in 1.7 s, Type II (BBC, EBU) 80%
  of a 10 ms burst and 24 dB in 2.8 s; alignment and 0 VU are a sine peaking
  at −18 dBFS.
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
- **Band correlation** (Stereo Bands): the Correlation above, per octave
  band (31 Hz to 16 kHz; each band Left and Right through the same 4th-order
  Butterworth high- and low-pass at its edges), averaged with τ = 300 ms by
  default. **Width** is `S²/(M²+S²)` with `M = (L+R)/2`, `S = (L−R)/2`: 0 mono,
  ½ unrelated, 1 opposite polarity. Below −80 dBFS a band has neither.
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
- **Before each release (from v0.1.0 on):** the official EBU loudness test
  set (Tech 3341 and 3342) at 48 kHz, every stereo case with the tolerances
  above. The EBU files may not be redistributed, so they're downloaded by
  hand and never committed. Case 6 of Tech 3341 is 5.0 and 5.1 surround, which the stereo
  Loudness Meter doesn't measure.

There's no official certification, so there's no badge.

## Known limits

- **192 kHz:** true peak isn't oversampled there, so it's only a sample peak.
  The Loudness Meter then labels it **Peak** instead of **TP**, so it never
  claims more than it measures.
- **System Capture** measures what the system mixer plays. For levels
  straight from a DAW track, use a [Send Plugin](send-plugins.md).
- A Spectrum sine half a bin off-centre reads up to 1.42 dB low (the Hann
  window's scalloping loss).
