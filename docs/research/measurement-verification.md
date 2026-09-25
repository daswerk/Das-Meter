# How to verify loudness, peak and stereo measurements against the standards

Research for [How to verify loudness, peak and stereo measurements against the standards](https://github.com/daswerk/Das-Meter/issues/18). This file gathers facts. It does **not** choose a verification approach; that is [Choose how the measurements are verified](https://github.com/daswerk/Das-Meter/issues/23).

Researched 2026-09-24. Terms follow [CONTEXT.md](../../CONTEXT.md): the **Loudness Meter** shows peak, RMS and LUFS; the **Stereometer** shows the stereo image and the phase correlation; the **Spectrum** is a Meter with a **Channel View**.

## Sources and what could not be reached

- **Blocked from the research sandbox:** `tech.ebu.ch` (R 128, Tech 3341, Tech 3342, the loudness test set and its terms of use), `www.itu.int` (BS.1770, BS.2217), `docs.rs`, `crates.io` web pages, `aes.org`, `wikipedia.org` and a third-party mirror of BS.1770-4. **No standard's text was read first-hand.**
- **Read first-hand:** the source, tests, CI and licence of the `ebur128` crate ([sdroege/ebur128](https://github.com/sdroege/ebur128), cloned at `ef08d2b`, 2025-02-23), of the C library it ports ([jiixyj/libebur128](https://github.com/jiixyj/libebur128)), and of [pyloudnorm](https://github.com/csteinmetz1/pyloudnorm). `cargo info ebur128` (crates.io index) for the current release.
- **Measured locally:** a small CPU benchmark of `ebur128` 0.1.10, and window-function figures computed from their formulas (both scripts are described below).
- Labels: **(unverified)** = not checked against the owning primary source; **(inference)** = my conclusion from what was read; **(second-hand)** = read in a third party's repository or a search-engine summary, not in the standard.

## Summary

| Area | Reference | Official test material | Tolerance |
|---|---|---|---|
| LUFS (M, S, I) | ITU-R BS.1770 (-5, 11/2023 is the newest revision seen); EBU Tech 3341 "EBU Mode" | EBU loudness test set v5 (`seq-3341-1` … `-14`); ITU-R BS.2217 compliance files | ±0.1 LU (second-hand, and in `ebur128`'s tests) |
| Loudness range | EBU Tech 3342 | `seq-3342-1` … `-6` | ±1 LU (same) |
| True peak | BS.1770 Annex 2; Tech 3341 | `seq-3341-15` … `-23` | +0.2 / −0.4 dB (same) |
| Sample peak, RMS | No single standard; AES17 (unverified) defines dB FS on a full-scale sine | None official | — |
| Correlation | Pearson correlation of L and R (unverified which standard fixes the averaging time) | None official | — |
| Spectrum | Window-function maths (Harris 1978) | Synthetic sines | Computed below |

## 1. LUFS: BS.1770, R 128, Tech 3341

### What the standards define (unverified; the texts were blocked)

- **ITU-R BS.1770** is "Algorithms to measure audio programme loudness and true-peak audio level". Search results list revisions -1 (2007), -2 (03/2011), -3 (08/2012), -4 (10/2015) and **-5 (11/2023)**. I could not open the ITU page to confirm -5 is still the newest in Sep 2026 (unverified).
- The measurement is a K-weighting filter (a high-shelf "pre-filter" plus a high-pass "RLB" filter), mean square per channel, channel weights (1.0 for L/R/C, 1.41 for the surround channels), summed, then `−0.691 + 10·log10(sum)`. The same constants are in `ebur128`: `10 * log10(energy) - 0.691` ([src/utils.rs](https://github.com/sdroege/ebur128/blob/main/src/utils.rs)) and `channel_sum *= 1.41` for surround ([src/filter.rs](https://github.com/sdroege/ebur128/blob/main/src/filter.rs)).
- BS.1770 publishes the K-weighting coefficients for **48 kHz** only (unverified). Other sample rates need coefficients derived from the analogue prototype. `ebur128` does this at runtime from centre frequency, gain and Q: `f0 = 1681.97…`, `G = 3.9998…`, `Q = 0.7071…` for the shelf and `f0 = 38.135…`, `Q = 0.5003…` for the high-pass, with `K = tan(π·f0/rate)` ([src/filter.rs `filter_coefficients`](https://github.com/sdroege/ebur128/blob/main/src/filter.rs)).
- **Integrated loudness gating** (BS.1770-2 onwards): 400 ms blocks with 75 % overlap, an absolute gate at −70 LUFS, then a relative gate 10 LU below the ungated loudness. `ebur128` uses the same numbers: 400 ms blocks with "75% overlap" ([src/ebur128.rs](https://github.com/sdroege/ebur128/blob/main/src/ebur128.rs)), `relative_gate = -10.0` and `-70.0` ([src/history.rs](https://github.com/sdroege/ebur128/blob/main/src/history.rs)).
- **EBU Tech 3341 ("EBU Mode")** defines three time scales: Momentary **M** (sliding 400 ms window, ungated), Short-term **S** (sliding 3 s window, ungated), Integrated **I** (gated as above). It also sets an update rate of at least 10 Hz and two display scales, "EBU +9" and "EBU +18" (unverified). `ebur128` names the same modes `M`, `S`, `I` and documents M as "last 400ms".
- **EBU R 128** sets the normalisation target: −23.0 LUFS integrated, ±0.5 LU normal tolerance (±1 LU for live), maximum true peak −1 dBTP (unverified). R 128 v4.0 is the version returned by searches (`r128v4_0.pdf`).

### Official test material

**EBU loudness test set** (current: v5, zip `ebu-loudness-test-setv05.zip`)

- Contents: "70 audio files meant to test loudness equipment for compliance with EBU Tech 3341 and EBU Tech 3342". v5 adds a mono noise signal for setting the reference listening level of Tech 3343 (second-hand, search summary of the EBU publication page).
- Download: `https://tech.ebu.ch/files/live/sites/tech/files/shared/testmaterial/ebu-loudness-test-setv05.zip`. This URL is in `ebur128`'s CI ([.github/workflows/ebur128.yml](https://github.com/sdroege/ebur128/blob/main/.github/workflows/ebur128.yml)). A 2026 third-party pull request reports that a scripted download got "HTTP 403 and a Cloudflare browser challenge" ([narration-utils PR 443](https://github.com/countrymanprime/narration-utils/pull/443)) (second-hand).
- **Licence / terms of use (second-hand, important):** the EBU publishes "Use of EBU audio test sequences" terms (July 2019, v1.0). According to the same pull request, they forbid "copying, publishing or distributing the sequences", limit use to "assessing equipment in internal R&D", and exclude "business, commercial or for-profit use". I could not read the terms myself (tech.ebu.ch blocked), so this is **unverified**. What is verified is how the projects behave: `ebur128`, `libebur128` and that project **do not commit the files**. They download them at CI time or ask the developer to download them by hand, and commit only the expected values.
- **Sample rate (inference):** every reference test in `ebur128` and `libebur128` opens the files at **48 kHz** (`48_000` is passed for every file in [tests/reference_tests.rs](https://github.com/sdroege/ebur128/blob/main/tests/reference_tests.rs)). The set therefore appears to test 48 kHz only. It does not show whether a meter is right at 44.1, 96 or 192 kHz.

**Tech 3341 minimum-requirement tests, with expected results and tolerances.** Copied from the README of [lcweden/loudness-worklet](https://github.com/lcweden/loudness-worklet) (second-hand). The same targets and tolerances are hard-coded in `ebur128`'s and `libebur128`'s tests:

| Signal | Expected |
|---|---|
| seq-3341-1 | M, S, I = −23.0 ±0.1 LUFS |
| seq-3341-2 | M, S, I = −33.0 ±0.1 LUFS |
| seq-3341-3, -4, -5 | I = −23.0 ±0.1 LUFS (gating tests) |
| seq-3341-6 | I = −23.0 ±0.1 LUFS (5.0 / 5.1 channels) |
| seq-3341-7, -8 | I = −23.0 ±0.1 LUFS (programme material; files shared with 3342-5/-6) |
| seq-3341-9 | S = −23.0 ±0.1, constant after 3 s |
| seq-3341-10-* | max S = −23.0 ±0.1 for each segment |
| seq-3341-11 | max S = −38, −37, … −19 ±0.1, successive values |
| seq-3341-12 | M = −23.0 ±0.1, constant after 1 s |
| seq-3341-13-* | max M = −23.0 ±0.1 for each segment |
| seq-3341-14 | max M = −38, −37, … −19 ±0.1, successive values |
| seq-3341-15 … -18 | max true peak = −6.0 dBTP, +0.2 / −0.4 dB |
| seq-3341-19 | max true peak = +3.0 dBTP, +0.2 / −0.4 dB |
| seq-3341-20 … -23 | max true peak = 0.0 dBTP, +0.2 / −0.4 dB |

**ITU-R Report BS.2217** "Compliance material for Recommendation ITU-R BS.1770" (latest seen: BS.2217-2, 10/2016). It holds a table of compliance files; "compliant meters will give the results indicated in the table to a tolerance of ±0.1 LKFS" (second-hand, search summary of the ITU PDF). pyloudnorm (MIT) **commits** a subset of these files to its repository (`tests/data/1770-2_Comp_*.wav`, `1770-2_Conf_*.wav`) and tests them at ±0.1 ([tests/test_loudness.py](https://github.com/csteinmetz1/pyloudnorm/blob/master/tests/test_loudness.py)). Their licence was not found in that repository (unverified whether redistribution is allowed).

## 2. Loudness range: Tech 3342

- LRA is the spread between the 10th and 95th percentiles of the short-term (3 s) loudness distribution. Values below an absolute gate of −70 LUFS and a relative gate 20 LU below the gated level are ignored (unverified in the standard). `ebur128` uses `-20.0` dB and percentiles `0.1` / `0.95` ([src/history.rs](https://github.com/sdroege/ebur128/blob/main/src/history.rs)).
- Test signals and expected LRA, ±1 LU (second-hand, same table source; also hard-coded in `ebur128`): seq-3342-1 = 10, -2 = 5, -3 = 20, -4 = 15, -5 = 5, -6 = 15 LU.

## 3. True peak: BS.1770 Annex 2

- **Method (unverified in the standard; confirmed by several secondary sources):** oversample at least 4× (48 → 192 kHz) with an interpolating FIR filter; Annex 2 gives an example 48-tap, 4-phase filter. The standard says "higher sampling rates and over-sampling ratios are preferred", and it describes a worst-case "under-read" that remains after 4× oversampling (Appendix 1 to Annex 2). Tech 3341 says the total true-peak error, including filter pass-band ripple and that under-read, must stay within the tolerances of tests 15–23 (second-hand, search summary of Tech 3341).
- **Tolerance:** +0.2 / −0.4 dB on seq-3341-15 … -23 (see the table above).
- **What `ebur128` does** ([src/true_peak.rs](https://github.com/sdroege/ebur128/blob/main/src/true_peak.rs), [src/interp.rs](https://github.com/sdroege/ebur128/blob/main/src/interp.rs)):
  - Oversampling is **4× below 96 kHz, 2× from 96 kHz up to below 192 kHz, and none at 192 kHz or above**. At 192 kHz, `TruePeak::new` returns `None`, and `true_peak()` returns the larger of sample peak and true peak, so it reports the **sample peak** there. The doc comment says the same: "Will oversample 4x for sample rates < 96000 Hz, 2x for sample rates < 192000 Hz and leave the signal unchanged for 192000 Hz."
  - The interpolator is its own **48-tap Hann-windowed sinc**, not the coefficient table printed in BS.1770 (inference: the code computes `w * sin(m·π/F)/(m·π/F)` with a Hann window).
  - The doc comment warns: "Uses an implementation defined algorithm … Do not try to compare resulting values across different versions of the library."
  - A `precision-true-peak` feature "increases the precision of true-peak calculation slightly, but causes a significant performance-hit" unless built with `-C target-feature=+fma` ([Cargo.toml](https://github.com/sdroege/ebur128/blob/main/Cargo.toml)).
  - Its reference test accepts true peak in `expected − 0.4 ..= expected + 0.2` dB. That is the Tech 3341 window.

## 4. The `ebur128` crate

| Fact | Detail | Source |
|---|---|---|
| Version | 0.1.10 (released 2024-10-26); last commit on `main` 2025-02-23 | `cargo info ebur128`, CHANGELOG, git log |
| Licence | **MIT**, compatible with MIT OR Apache-2.0 | Cargo.toml, LICENSE, README |
| Origin | "Rust port of the libebur128 C library, produces the same results as the C library and has comparable performance" | README |
| Runtime deps | `bitflags`, `smallvec`, `dasp_sample`, `dasp_frame` | Cargo.toml |
| MSRV | Rust 1.60, checked in CI | Cargo.toml, CI |
| Features | M, S, I, LRA, sample peak, true peak, a histogram mode, `loudness_window(ms)` for custom windows, `seed_frames_*` and `*_multiple` for combining parallel analysers, a C API compatible with libebur128 | README, src/ebur128.rs |
| Conformance claim | "passes all tests defined in EBU - TECH 3341 and EBU - TECH 3342" | README |
| Conformance tests | `tests/reference_tests.rs` covers **seq-3341-1 … -23** (including -6 in 5- and 6-channel form) and **seq-3342-1 … -6**, all at 48 kHz. Tolerances: ±0.1 LU for M/S/I, ±1 LU for LRA, +0.2/−0.4 dB for true peak. It runs both the normal and the histogram mode. | tests/reference_tests.rs |
| CI | GitHub Actions on stable, beta and nightly: downloads the EBU zip, then `cargo test --features c-tests,internal-tests,reference-tests`. Also: rustfmt, clippy `-D warnings`, the C API under valgrind, MSRV check. The reference tests run only with `--features reference-tests` and the files in `tests/reference_files`. | .github/workflows/ebur128.yml |
| Other tests | Quickcheck tests compare the Rust code with the C original on random signals, at random sample rates and channel counts. | src/*.rs `#[cfg(feature = "c-tests")]` |
| Sample rates | Any rate from 16 Hz to 2 822 400 Hz is accepted (`MAX_RATE`), with K-weighting recomputed per rate. True peak is 4× / 2× / none as above. The EBU conformance tests run at 48 kHz only. | src/ebur128.rs, src/true_peak.rs |
| Input formats | i16, i32, f32, f64, interleaved or planar | src/ebur128.rs |

**CPU cost, measured here** (inference: one machine, rough): `ebur128` 0.1.10, release build, 60 s of stereo f32 (sine plus noise) fed in 10 ms blocks, reading momentary loudness after each block. Intel Xeon @ 2.10 GHz, 4 vCPU sandbox. Figures are % of one core in real time:

| Rate | M+S+I | + LRA + sample peak | everything (`Mode::all()`: + true peak + histogram) |
|---|---|---|---|
| 44.1 kHz | 0.27 % | 0.27 % | 0.41 % |
| 48 kHz | 0.31 % | 0.34 % | 0.44 % |
| 96 kHz | 0.56 % | 0.70 % | 0.90 % |
| 192 kHz | 1.24 % | 1.24 % | 1.51 % (true peak here = sample peak) |

So one stereo instance costs about 0.3–1.5 % of one core, rising roughly linearly with sample rate. The EBU test files themselves could not be downloaded (tech.ebu.ch blocked), so the conformance tests were **not** re-run here.

## 5. Sample peak and RMS

(All unverified: aes.org and IEC pages were not reachable. These are widely documented conventions, not checked against the standards' text.)

- **Sample peak:** the largest absolute sample value, in dBFS = `20·log10(|x|max)`. It under-reads the reconstructed analogue peak by up to about 3 dB for signals near Nyquist (for example a sine at fs/4 with a 45° phase offset reads −3.01 dB below its true peak). That under-read is why true peak exists.
- **RMS level:** `20·log10(sqrt(mean(x²)))`. A full-scale sine has RMS = 1/√2, so it reads **−3.01 dBFS** without a correction. **AES17** defines "dB FS" relative to the RMS of a full-scale sine, so a full-scale sine reads **0 dB FS**. Meters that follow it add **+3.01 dB** to the raw RMS (unverified). Meters differ on this, so any RMS test has to state which convention it uses.
- **RMS window:** there is no single standard. Common choices are a 300 ms integration (VU-like ballistics, IEC 60268-17 for VU meters) and sliding windows of 50–3000 ms. PPM ballistics are in IEC 60268-10; IEC 60268-18 covers digital peak-level meters (all unverified). BS.1770 "loudness" is itself a K-weighted mean square over 400 ms (M) or 3 s (S).
- **Checks that follow from the maths (inference):** a full-scale 997 Hz sine should read sample peak ≈ 0.00 dBFS, true peak ≈ 0.00 dBTP, RMS ≈ −3.01 dB (or 0.00 dB with the AES17 offset), and momentary loudness about −3.0 LUFS on one channel. pyloudnorm's own test expects −3.052 LUFS for its `sine_1000.wav`. Full-scale square wave: RMS = 0 dB. These can be generated at any sample rate without licensed files.

## 6. Stereometer correlation

(Unverified: no standard's text could be read.)

- **Formula:** the correlation coefficient `r = Σ(L·R) / sqrt(Σ(L²)·Σ(R²))`, in −1 … +1. +1 = mono / in phase, 0 = uncorrelated (for example independent noise), −1 = one channel inverted. The sums are usually running averages (exponential or sliding window) over the same time span for all three terms. The DC mean is usually not subtracted.
- **Averaging time:** I found no primary source that fixes it. IEC 60268-18 is often cited for peak meters, not for correlation meters (unverified). Commercial and open-source correlation meters use integration times from about 100 ms to several hundred ms (unverified).
- **Checks that follow from the maths (inference):** identical L and R → +1; L = −R → −1; two independent noise signals → about 0, with a spread that shrinks as the window grows; L = sin, R = sin shifted by φ → cos φ (0 at 90°). Silent input gives 0/0, so the behaviour on silence has to be defined.

## 7. Spectrum: FFT magnitude and frequency accuracy

### Window figures (computed here from the window formulas, N = 4096; they agree with Harris, "On the use of windows for harmonic analysis with the DFT", Proc. IEEE 1978 (unverified))

| Window | Coherent gain | ENBW | Worst-case scalloping loss |
|---|---|---|---|
| Rectangular | 1.000 (0.00 dB) | 1.000 bin | 3.92 dB |
| Hann | 0.500 (−6.02 dB) | 1.500 bins | 1.42 dB |
| Hamming | 0.540 (−5.35 dB) | 1.363 bins | 1.75 dB |
| Blackman-Harris (4-term) | 0.359 (−8.90 dB) | 2.004 bins | 0.83 dB |
| Flat-top (SRS, 5-term) | 0.216 (−13.33 dB) | 3.770 bins | 0.01 dB |

- **Sine level:** a sine of amplitude A lands in the FFT at `|X| = A·N·CG/2`. Scale by `2/(N·CG)` to read a sine's amplitude correctly when it sits exactly on a bin. Between bins it reads low by up to the scalloping loss.
- **Noise level:** noise power per bin scales with ENBW, so broadband noise needs a different correction (divide by ENBW) from a sine. A Spectrum can be calibrated so that sines read correctly or so that noise density reads correctly, not both (inference).
- **Frequency accuracy:** bin spacing is `fs/N`. A peak's frequency can be refined between bins by interpolating neighbouring bins (for example parabolic interpolation on dB magnitudes). A test sine should be placed both on a bin and at half a bin (inference).
- **Tilt / slope:** pink noise falls 3 dB/octave in a constant-bandwidth FFT, so analysers offer a "slope" that adds a gain rising with frequency to flatten it. **4.5 dB/octave** is the default slope in some well-known analysers (Voxengo SPAN, and, as often said, in FabFilter Pro-Q) (unverified). 3 dB/octave makes pink noise flat. The tilt is usually pivoted at 1 kHz (unverified). Test (inference): with slope S dB/oct pivoted at f0, a sine at 2·f0 reads S dB higher than the same sine at f0.
- These checks need only synthetic sines and noise, so they raise no licence questions.

## 8. Licence notes for Das-Meter (MIT OR Apache-2.0)

- `ebur128` and `libebur128`: MIT. Compatible.
- pyloudnorm: MIT (code). The licence of the BS.2217 WAV files it carries was not found.
- EBU test set: reportedly no redistribution and "internal R&D" use only (second-hand). The projects read here all keep the files out of their repositories.
- Test code, expected values and synthetic signals Das-Meter writes itself raise no licence questions (inference).
- GPL meters (for example x42-meters, GPL) were not read for code. CLAUDE.md forbids copying GPL code.
