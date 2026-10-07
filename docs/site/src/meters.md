# Meters

Das-Meter has seven kinds of Meter. Right-click one for its settings; the settings
panel has the same settings for every Meter.

## Waveform

The audio over time, coloured by frequency band (low, mid, high). Settings:
**Channels** (mono, Left/Right, or Mid/Side), time span (1–30 s), display gain
and the two band crossovers.

## Spectrum

How loud each frequency is. Settings: **Channels**, **FFT size**, window,
**Slope** (0, 3, 4.5 or 6 dB per octave, pivoted at 1 kHz), **Smoothing**,
**Style** (a line, or bars at 3, 6 or 12 per octave), attack and release,
**Peak hold**, **Peak line**, and the shown frequency and level ranges. With
**Peak line** on, the loudest peak's level, frequency and note stay at the top
right, and a thin line runs from the peak to them.
Turn on **Mouse readout** to read any frequency and its note under the mouse.
Drag a box over the Spectrum to zoom into it: a zoom window opens in the
other half of the Meter, showing the box's frequencies and levels larger, with
its own scale and a finer frequency resolution (the largest FFT size). Its ×
closes it. A tone keeps its level in the zoom window, but noise looks lower
there, since each point covers fewer frequencies.
While the mouse is over the Spectrum it falls slower and holds its peaks
(**Slow down under the mouse**), and the peaks that keep sounding get a dot
with their note; the one nearest the mouse also shows its frequency and level.
Only peaks that stand out for a couple of seconds are marked, and a dot stays
where it was found. The zoom window marks its peaks the same way.
**Colour ▸ Steady harmonics** makes harmonics that keep sounding glow brighter.
Their colours, **Spectrum peak dots** and **Spectrum harmonics**, are with
the Spectrum's other colours in the Theme and the Meter's own colours. With
two traces (Left and Right, or Mid and Side), the second one's colour is
**Spectrum R / Side**.

## Loudness Meter

How loud the audio is: momentary (M), short-term (S) and integrated (I) LUFS,
loudness range (LRA), true peak (TP), and the peak-to-loudness ratios **PLR**
(true-peak maximum − integrated) and **PSR** (the last 3 s' true peak −
short-term). By default it shows numbers only: two big numbers, the
loudness (short-term LUFS unless **Big reading** picks momentary, integrated or
true peak) and the sample peak in dBFS, with the other readings as plain rows.
**Thin bars** adds a thin bar beside each big number. Turn on **Show bars** for
the peak and RMS bars beside (or under) the numbers. Where there's room, the **loudness
graph** shows the LUFS reading over the last 10 to 120 s, with the target
and integrated lines. Settings: a **Target** loudness, **Show true peak**,
**Show LRA**, **Show PLR and PSR**, **Loudness graph** and its span, the RMS
window, peak hold (or **Hold peaks until reset**) and the bar range. Click the Meter to
reset its integrated loudness, LRA and maxima.

See [Measurements](measurements.md) for exactly what each number is.

## Stereometer

Where the sound sits between left and right, with the phase **correlation**
under it (+1: mono, 0: unrelated, −1: out of phase) and an optional **Balance
bar**. Settings: the view, **Scale** (auto or fixed gain), persistence and
the correlation's averaging time. The scope stretches to whatever shape the
Meter has; in a wide, low Meter the bars stand upright beside it.

## Cepstrum

The cepstrum of the mono sum: evenly spaced partials of a harmonic sound
(a voice, a bass, most instruments) make a peak at the length of their period,
and echoes make peaks at their delay. The axis runs from short periods (high
pitch) on the left to long ones (low pitch) on the right, labelled in Hz.
With **Show pitch** on, a line marks the pitch found, with its frequency and
note; noise and chords give "No pitch". Under the mouse it shows the pitch
and note of the period there. Settings: **FFT size**, the lowest and
highest pitch to look for (50 to 1000 Hz by default) and the smoothing.

To show it, right-click a Meter and choose **Show ▸ Cepstrum**.

## Spectrogram

The Spectrum over time: frequency from the bottom up, time scrolling to the
left with the newest audio at the right edge, and louder shown brighter, in
the Theme's Spectrum colours up to its text colour. Settings: **Span** (5, 10,
20 or 30 s), **Frequency scale**, and in the settings panel the FFT size,
window, slope and the frequency and level ranges.
Drag a box over it to zoom into the box's frequencies: a zoom window opens in
the other half of the Meter at the finest frequency resolution, scrolling three
times slower so the detail shows. Its × closes it.

To show it, right-click a Meter and choose **Show ▸ Spectrogram**, or
**Add Meter ▸ Spectrogram**.

## Phase Scope

The waveform over one **Cycle**, one beat or one bar, held still so every
beat lands in the same place: a kick on the beat stays put, and you can watch
how its attack and tail sit against the bass. The newest Cycle is drawn sharp
with the few before it fading behind. It's drawn as it plays, like a
scope's beam sweeping across, so a fade-out fades smoothly.

On a Send Plugin it follows the DAW: its tempo, its beat and, for a bar
Cycle, its time signature. When the DAW stops, it keeps running at the last
tempo and locks on again when you press play. On System Capture there's no
DAW to follow, so it runs at a **Tempo** you type in (120 BPM by default), as
it does for a Send Plugin from an older version or a host that doesn't say
its tempo ("No tempo from …"). The tempo it follows is shown at the bottom
left.
All its options are in its right-click menu:

- **Cycle**: one beat, or one bar with a line on each beat. A bar follows the
  DAW's time signature, or is 4/4 when there's no DAW to say.
- **Tempo**, with a **Tap** button: tap along twice or more and the taps set
  it. A pause of two seconds starts a fresh count.
- **Look**: a line, or filled to the centre.
- **Channels**: mono, Left and Right, or Mid and Side, each in its own band.
- **Gain**: auto (the loudest point fills the Meter) or manual, in dB.
- **Steadiness**: the newest Cycle sharp with the previous few fading behind
  it, or the last few Cycles averaged, which holds a repeating pattern steady
  and smooths out a one-off.

### Overlay Source

While listening to Send Plugins, pick an **Overlay Source** in the Phase
Scope's menu: a second Send Plugin from the same DAW, say the bass track
under a Phase Scope on the kick. It's drawn over the main trace in its own
Send Plugin's colour, placed by its own song position, so the two line up
as they do in the song, with their sum (what's actually left) drawn as its
own waveform on top (**Show sum**, on by default). A coloured
lane under the Cycle shows how their lows move at each point: green where
they push together, red where they cancel, stronger where they're louder.
The large number in the top right is their correlation below the
**Correlation below** frequency (150 Hz by default), with what it means in
words: +1 when the lows move together, −1 when one is the other upside
down. It's blank in silence, and the first thing left out when the Meter is
small. **Offset** nudges it
a few milliseconds later (+) or earlier (−). Turn on **Suggestions** and,
when the lows don't fit, it says what would help under the number: flip the
Overlay Source's polarity, move it by so many milliseconds (set that as the
Offset to hear it), or pitch it to match the other's low end. If its Send Plugin goes away,
the Phase Scope says "Waiting for …" until it's back. The pick and offset
are saved in Presets. With System Capture there's only the one Source, so
the option is greyed out. An Overlay Source is compared in mono, so while one
is shown the main trace is mono too.

When it's very small the readouts go first, so the traces keep the room.

To show it, right-click a Meter and choose **Show ▸ Phase Scope**, or
**Add Meter ▸ Phase Scope**.

## Small sizes

When a Meter gets small, it shows less (fewer labels and readings) rather than
letting them overlap.
