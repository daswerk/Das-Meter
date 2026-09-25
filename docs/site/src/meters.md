# Meters

Das-Meter has five kinds of Meter. Right-click one for its settings; the settings
panel has the same settings for every Meter.

## Waveform

The audio over time, coloured by frequency band (low, mid, high). Settings:
**Channels** (mono, Left/Right, or Mid/Side), time span (1–30 s), display gain
and the two band crossovers.

## Spectrum

How loud each frequency is. Settings: **Channels**, **FFT size**, window,
**Slope** (0, 3, 4.5 or 6 dB per octave, pivoted at 1 kHz), **Smoothing**,
**Style** (a line, or bars at 3, 6 or 12 per octave), attack and release,
**Peak hold**, **Peak line**, and the shown frequency and level ranges. The
**peak line** marks the loudest peak as drawn, with its frequency and note.
Hover over the Spectrum to read any frequency and its note.

## Loudness Meter

How loud the audio is: momentary (M), short-term (S) and integrated (I) LUFS,
loudness range (LRA), true peak (TP), and the peak-to-loudness ratios **PLR**
(true-peak maximum − integrated) and **PSR** (the last 3 s' true peak −
short-term), with peak and RMS bars. Where there's room, the **loudness
graph** shows the LUFS bar's reading over the last 10 to 120 s, with the target
and integrated lines. Settings: a **Target** loudness, **Show true peak**,
**Show LRA**, **Show PLR and PSR**, **Loudness graph** and its span, the RMS
window, peak hold (or **Hold peaks until reset**) and the bar range. Click the Meter to
reset its integrated loudness, LRA and maxima.

See [Measurements](measurements.md) for exactly what each number is.

## Stereometer

Where the sound sits between left and right, with the phase **correlation**
under it (+1: mono, 0: unrelated, −1: out of phase) and an optional **Balance
bar**. Settings: the view, **Scale** (auto or fixed gain), persistence and
the correlation's averaging time.

## Cepstrum

The cepstrum of the mono sum: evenly spaced partials of a harmonic sound
(a voice, a bass, most instruments) make a peak at the length of their period,
and echoes make peaks at their delay. The axis runs from short periods (high
pitch) on the left to long ones (low pitch) on the right, labelled in Hz.
With **Show pitch** on, a line marks the pitch found, with its frequency and
note; noise and chords give "No pitch". Settings: **FFT size**, the lowest and
highest pitch to look for (50 to 1000 Hz by default) and the smoothing.

To show it, right-click a Meter and choose **Show ▸ Cepstrum**.

## Small sizes

When a Meter gets small, it shows less (fewer labels and readings) rather than
letting them overlap.
