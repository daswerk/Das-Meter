# The Loudness Meter's scales: VU, PPM and K-System ballistics

The Loudness Meter's bars can use a **Scale** (#104): **Peak** (sample peak and RMS in dBFS, as before), **VU**, a **PPM** (DIN, Nordic, BBC or EBU) or the **K-System** (K-12, K-14, K-20). This reverses the v1 spec's "no VU" (#25). Like ADR 0005's measurements, each ballistic is defined here and checked by a test (`crates/analysis/tests/scales_golden.rs`). The figures come from the published standards as summarised in public references; no code was taken from other meters (x42-meters is GPL: ideas only).

## Definitions

Every ballistic reading is kept in **dBFS of a sine's peak**: a steady sine reads its peak level on every scale, so the scales differ only in their timing and where their numbers sit.

- **VU** (IEC 60268-17, ANSI C16.5): the full-wave rectified signal through a second-order filter that first reaches **99% of a step in 300 ms** and **overshoots by 1.25%** (the standard asks 1–1.5%): damping 0.8127, natural frequency 13.512 rad/s. Its average is scaled by π/2 so a sine reads its peak. **0 VU** is a sine peaking at **−18 dBFS** by default (the common "+4 dBu = −18 dBFS"; adjustable from −24 to −8). Scale −20 to +3 VU.
- **PPM** (IEC 60268-10), quasi-peak, full-wave: the reading charges toward the signal when it's higher, with a one-pole attack, and otherwise falls straight in dB.
  - **Type I, DIN and Nordic:** a 5 ms burst of 5 kHz reads **80%** (−1.9 dB) of the steady tone (attack time constant 1.2435 ms); it falls **20 dB in 1.7 s**. DIN: −50 to +5 dB, its 0 at **−9 dBFS** (the EBU permitted maximum, 9 dB over alignment). Nordic: −36 to +9 dB, 0 ("TEST") at alignment.
  - **Type II, BBC and EBU:** a 10 ms burst reads **80%** (attack 2.4867 ms); it falls **24 dB in 2.8 s**. BBC: marks 1 to 7, **4 dB apart, mark 4 at alignment**. EBU: −12 to +12 dB, 0 at alignment.
  - **Alignment** is a sine peaking at **−18 dBFS** (EBU R68).
- **K-System** (Bob Katz): the Meter's RMS (AES17 by default, 300 ms window) with **0 at −12, −14 or −20 dB**, its top at 0 dBFS, and the sample peak over it. The bar turns the over-target colour above 0 and the clip colour above +4.

Sources disagree on Type I's fall (1.5 s or 1.7 s for 20 dB); we use 1.7 s. The attack constants are calibrated so the burst readings hold at any sample rate.

## Consequences

- Only the bars use a Scale; the numbers stay LUFS, true peak and sample peak. On a standard scale the bars are the two channels alone, on that scale's range and numbers; the LUFS bar shows only on Peak.
- The **Needle Meter** (#106) uses the same VU and PPM ballistics on a dial: a VU's spaced by voltage as real ones are, a PPM's by dB; its red zone starts at 0 VU or at the PPMs' permitted maximum (−9 dBFS).

Decided in [Meter scales](https://github.com/daswerk/Das-Meter/issues/104).
