//! The Phase Scope: the waveform over one Cycle (a beat or a bar), held still
//! so every beat lands in the same place.
//!
//! Each Source's audio goes into a ring with a beat clock beside it: anchors
//! that tie a frame to a song position (in quarter notes) and a tempo. With
//! DAW timing the anchors follow the DAW; without it, or while the DAW is
//! stopped, the clock runs on at the last (or typed-in) tempo. Whenever a
//! whole Cycle has arrived it is cut from the ring into columns, and the
//! view shows the newest Cycle with the few before it.

use std::collections::VecDeque;
use std::time::Duration;

use dasmeter_analysis::ChannelView;

use crate::meters::{CycleLength, PhaseScopeMeterSettings, Steadiness};
use crate::sources::Timing;
use crate::theme::Colour;

/// Columns across one Cycle.
pub const COLUMNS: usize = 512;
/// Earlier Cycles drawn fading behind the newest.
pub const TRAIL: usize = 3;
/// Cycles averaged when steadiness is set to average.
pub const AVERAGED: usize = 8;
/// The longest Cycle kept, in seconds: a bar of 4/4 at 30 BPM.
const MAX_SECONDS: f64 = 8.0;
/// Frames a Cycle waits after it ends before it is cut, so the Overlay
/// Source's audio for it has arrived too.
const MARGIN_SECONDS: f64 = 0.03;
/// A DAW song position further than this from where the clock expects it
/// (in quarter notes) is a jump: the clock locks on again.
const JUMP_BEATS: f64 = 0.05;
/// What auto gain scales the loudest point to.
const AUTO_GAIN_PEAK: f32 = 0.9;

/// One trace across the Cycle: each column's lowest and highest value, −1 to
/// 1 after gain, rounded to 0.01.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ScopeTrace {
    pub min: Vec<f32>,
    pub max: Vec<f32>,
}

/// What a Phase Scope shows.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PhaseScopeView {
    /// The newest Cycle (or the average of the last few): one trace for
    /// mono, two for Left and Right or Mid and Side.
    pub traces: Vec<ScopeTrace>,
    /// The Cycles before it, oldest first, drawn fading. Empty when averaging.
    pub trail: Vec<Vec<ScopeTrace>>,
    /// Where the beats fall inside the Cycle, 0–1 across (the Cycle's own
    /// start left out).
    pub beat_lines: Vec<f32>,
    /// The tempo the Cycle follows, to 0.1 BPM.
    pub tempo: f32,
    /// Whether it follows the DAW's tempo and song position; otherwise the
    /// typed-in tempo.
    pub following_daw: bool,
    /// Whether the Source says where its DAW is (a Send Plugin in a host
    /// that gives its transport), playing or not.
    pub said_tempo: bool,
    /// The Send Plugin's colour, for the main trace (none on System Capture).
    pub colour: Option<Colour>,
    /// The Overlay Source, when there is one.
    pub overlay: Option<ScopeOverlay>,
    /// Shown in place of the tempo readout: why the DAW's tempo isn't followed.
    pub note: Option<String>,
}

/// The Overlay Source drawn over the main Source, and how the two combine.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ScopeOverlay {
    /// Its mono trace.
    pub trace: ScopeTrace,
    pub colour: Option<Colour>,
    /// The mono sum of both.
    pub sum: ScopeTrace,
    /// Per column, how strongly the two push in opposite directions, 0–1.
    pub cancel: Vec<f32>,
    /// How well the two agree below the cut-off, −1 to 1 to 0.01; `None` in
    /// silence.
    pub correlation: Option<f32>,
}

/// A song position at a frame.
#[derive(Clone, Copy, Debug)]
struct Anchor {
    frame: u64,
    /// Quarter notes.
    beats: f64,
    /// Quarter notes per minute.
    tempo: f64,
}

/// Where the Cycles fall: from `origin` (a bar line), every `beat` quarter
/// notes a beat, `per_bar` beats a bar.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Grid {
    origin: f64,
    beat: f64,
    per_bar: u32,
}

impl Grid {
    const FREE: Grid = Grid {
        origin: 0.0,
        beat: 1.0,
        per_bar: 4,
    };

    fn cycle(&self, length: CycleLength) -> f64 {
        match length {
            CycleLength::Beat => self.beat,
            CycleLength::Bar => self.beat * f64::from(self.per_bar),
        }
    }
}

/// One Source's recent audio and its beat clock.
struct Track {
    rate: f64,
    ring: Vec<[f32; 2]>,
    /// Frames received since the start.
    end: u64,
    /// The clock, oldest first; frames before the first are not on it.
    anchors: VecDeque<Anchor>,
    /// Whether the last block came with DAW timing while it played.
    following: bool,
    /// Whether the last block came with DAW timing at all.
    timed: bool,
    grid: Grid,
}

impl Track {
    fn new(rate: u32) -> Track {
        let capacity = (MAX_SECONDS * f64::from(rate)) as usize + 1;
        Track {
            rate: f64::from(rate),
            ring: vec![[0.0; 2]; capacity],
            end: 0,
            anchors: VecDeque::new(),
            following: false,
            timed: false,
            grid: Grid::FREE,
        }
    }

    fn oldest(&self) -> u64 {
        self.end.saturating_sub(self.ring.len() as u64)
    }

    fn sample(&self, frame: u64) -> [f32; 2] {
        if frame >= self.end || frame < self.oldest() {
            return [0.0; 2];
        }
        self.ring[(frame % self.ring.len() as u64) as usize]
    }

    /// The song position the clock gives `frame`.
    fn beats_at(&self, frame: u64) -> Option<f64> {
        let anchor = self.anchors.iter().rev().find(|a| a.frame <= frame)?;
        Some(anchor.beats + (frame - anchor.frame) as f64 / self.rate * anchor.tempo / 60.0)
    }

    /// The (fractional) frame at song position `beats`, if the clock covers it.
    fn frame_at(&self, beats: f64) -> Option<f64> {
        let first = self.anchors.front()?;
        if beats < first.beats {
            return None;
        }
        let anchor = self.anchors.iter().rev().find(|a| a.beats <= beats)?;
        Some(anchor.frame as f64 + (beats - anchor.beats) * 60.0 / anchor.tempo * self.rate)
    }

    fn tempo(&self) -> Option<f64> {
        self.anchors.back().map(|a| a.tempo)
    }

    /// Takes a block of interleaved stereo frames and moves the clock on:
    /// with the DAW while it plays, else at the DAW's tempo (stopped), else
    /// at `free`, the typed-in tempo. Returns whether the clock locked on
    /// afresh (play started, or the song position jumped).
    fn push(&mut self, frames: &[f32], timing: Option<Timing>, free: f64) -> bool {
        let start = self.end;
        let mut relocked = false;
        self.timed = timing.is_some();
        match timing.filter(|t| t.playing && t.tempo > 0.0) {
            Some(t) => {
                let expected = self.beats_at(start);
                if !self.following
                    || expected.is_none_or(|beats| (beats - t.beats).abs() > JUMP_BEATS)
                {
                    // What came before is from elsewhere in the song.
                    self.anchors.clear();
                    relocked = true;
                }
                self.anchors.push_back(Anchor {
                    frame: start,
                    beats: t.beats,
                    tempo: t.tempo,
                });
                let (num, den) = t.signature;
                self.grid = Grid {
                    origin: t.bar_start,
                    beat: 4.0 / f64::from(den.max(1)),
                    per_bar: u32::from(num.max(1)),
                };
                self.following = true;
            }
            None => {
                let tempo = timing.map(|t| t.tempo).filter(|&t| t > 0.0).unwrap_or(free);
                if timing.is_none() {
                    self.grid = Grid::FREE;
                }
                match self.beats_at(start) {
                    Some(beats) if self.tempo() != Some(tempo) => self.anchors.push_back(Anchor {
                        frame: start,
                        beats,
                        tempo,
                    }),
                    Some(_) => {}
                    None => self.anchors.push_back(Anchor {
                        frame: start,
                        beats: 0.0,
                        tempo,
                    }),
                }
                self.following = false;
            }
        }
        let capacity = self.ring.len() as u64;
        for frame in frames.chunks_exact(2) {
            self.ring[(self.end % capacity) as usize] = [frame[0], frame[1]];
            self.end += 1;
        }
        // Anchors the ring has left behind, keeping the one that covers its oldest frame.
        let oldest = self.oldest();
        while self.anchors.len() > 1 && self.anchors[1].frame <= oldest {
            self.anchors.pop_front();
        }
        relocked
    }

    /// The song position of the newest frame that's safe to show.
    fn ready_beats(&self) -> Option<f64> {
        let margin = (MARGIN_SECONDS * self.rate) as u64;
        self.beats_at(self.end.checked_sub(margin + 1)?)
    }
}

/// One Cycle cut into columns, before gain: per trace, the lowest and
/// highest value in each column.
#[derive(Clone, Debug, PartialEq)]
struct Cut {
    traces: Vec<(Vec<f32>, Vec<f32>)>,
}

/// The Phase Scope's analysis.
pub(crate) struct PhaseScope {
    rate: u32,
    settings: PhaseScopeMeterSettings,
    main: Track,
    /// The newest Cycle cut, by its number on the grid.
    shown: Option<i64>,
    /// Newest last.
    history: VecDeque<Cut>,
}

impl PhaseScope {
    pub fn new(rate: u32, settings: PhaseScopeMeterSettings) -> PhaseScope {
        PhaseScope {
            rate,
            settings,
            main: Track::new(rate),
            shown: None,
            history: VecDeque::new(),
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    pub fn set_settings(&mut self, settings: PhaseScopeMeterSettings) {
        let old = std::mem::replace(&mut self.settings, settings);
        // A new Cycle length, tempo or channel split: the Cycles kept no longer match.
        if old.cycle != settings.cycle
            || old.tempo != settings.tempo
            || old.channel_view != settings.channel_view
        {
            self.history.clear();
            self.shown = None;
        }
    }

    /// Takes the main Source's audio. Returns whether a new Cycle came in.
    pub fn process(&mut self, frames: &[f32], timing: Option<Timing>) -> bool {
        if self
            .main
            .push(frames, timing, f64::from(self.settings.tempo))
        {
            // Cycles are counted afresh from where the clock locked on.
            self.shown = None;
        }
        self.cut_ready()
    }

    /// Cuts every whole Cycle that's arrived since the last.
    fn cut_ready(&mut self) -> bool {
        let Some(ready) = self.main.ready_beats() else {
            return false;
        };
        let grid = self.main.grid;
        let length = grid.cycle(self.settings.cycle);
        let newest = ((ready - grid.origin) / length).floor() as i64 - 1;
        if self.shown.is_some_and(|shown| shown >= newest) {
            return false;
        }
        let start = grid.origin + newest as f64 * length;
        let Some(cut) = self.cut(start, length) else {
            return false;
        };
        self.shown = Some(newest);
        self.history.push_back(cut);
        while self.history.len() > AVERAGED.max(TRAIL + 1) {
            self.history.pop_front();
        }
        true
    }

    /// The Cycle from `start` (quarter notes), `length` long, if the ring
    /// still holds all of it since the clock last locked on.
    fn cut(&self, start: f64, length: f64) -> Option<Cut> {
        let track = &self.main;
        let first = track.frame_at(start)?;
        if first < track.oldest() as f64 {
            return None;
        }
        let view = self.settings.channel_view;
        let count = view.traces();
        let mut traces = vec![(vec![0.0f32; COLUMNS], vec![0.0f32; COLUMNS]); count];
        let mut from = first;
        for column in 0..COLUMNS {
            let to = track.frame_at(start + length * (column + 1) as f64 / COLUMNS as f64)?;
            let (a, b) = (
                from.floor() as u64,
                (to.floor() as u64).max(from.floor() as u64 + 1),
            );
            let (mut low, mut high) = ([f32::MAX; 2], [f32::MIN; 2]);
            for frame in a..b {
                let [l, r] = track.sample(frame);
                let split = view.split(l, r);
                for t in 0..count {
                    low[t] = low[t].min(split[t]);
                    high[t] = high[t].max(split[t]);
                }
            }
            for (t, (min, max)) in traces.iter_mut().enumerate() {
                min[column] = low[t];
                max[column] = high[t];
            }
            from = to;
        }
        Some(Cut { traces })
    }

    /// What the scope shows now.
    pub fn view(&self) -> PhaseScopeView {
        let s = &self.settings;
        let grid = self.main.grid;
        let cycles: Vec<&Cut> = self.history.iter().collect();
        let (newest, trail): (Option<Cut>, Vec<&Cut>) = match s.steadiness {
            Steadiness::Trail => {
                let newest = cycles.last().map(|&c| c.clone());
                let skip = cycles.len().saturating_sub(TRAIL + 1);
                let trail = cycles[skip..cycles.len().saturating_sub(1)].to_vec();
                (newest, trail)
            }
            Steadiness::Average => {
                let skip = cycles.len().saturating_sub(AVERAGED);
                (average(&cycles[skip..]), Vec::new())
            }
        };
        let count = s.channel_view.traces();
        let newest = newest.unwrap_or_else(|| Cut {
            traces: vec![(vec![0.0; COLUMNS], vec![0.0; COLUMNS]); count],
        });
        let all = std::iter::once(&newest).chain(trail.iter().copied());
        let gain = if s.auto_gain {
            let peak = all
                .flat_map(|c| &c.traces)
                .flat_map(|(min, max)| min.iter().chain(max))
                .fold(0.0f32, |peak, v| peak.max(v.abs()));
            if peak > 1e-6 {
                AUTO_GAIN_PEAK / peak
            } else {
                1.0
            }
        } else {
            10f32.powf(s.gain / 20.0)
        };
        let scale = |cut: &Cut| -> Vec<ScopeTrace> {
            cut.traces
                .iter()
                .map(|(min, max)| ScopeTrace {
                    min: min.iter().map(|&v| level(v * gain)).collect(),
                    max: max.iter().map(|&v| level(v * gain)).collect(),
                })
                .collect()
        };
        let beats = match s.cycle {
            CycleLength::Beat => 1,
            CycleLength::Bar => grid.per_bar,
        };
        PhaseScopeView {
            traces: scale(&newest),
            trail: trail.iter().map(|c| scale(c)).collect(),
            beat_lines: (1..beats).map(|b| b as f32 / beats as f32).collect(),
            tempo: self
                .main
                .tempo()
                .map_or(s.tempo, |t| ((t * 10.0).round() / 10.0) as f32),
            following_daw: self.main.following,
            said_tempo: self.main.timed,
            colour: None,
            overlay: None,
            note: None,
        }
    }
}

/// Each column's lowest and highest values averaged over `cuts`.
fn average(cuts: &[&Cut]) -> Option<Cut> {
    let first = cuts.first()?;
    let n = cuts.len() as f32;
    let mean = |t: usize, low: bool| -> Vec<f32> {
        (0..COLUMNS)
            .map(|i| {
                cuts.iter()
                    .map(|c| {
                        if low {
                            c.traces[t].0[i]
                        } else {
                            c.traces[t].1[i]
                        }
                    })
                    .sum::<f32>()
                    / n
            })
            .collect()
    };
    let traces = (0..first.traces.len())
        .map(|t| (mean(t, true), mean(t, false)))
        .collect();
    Some(Cut { traces })
}

/// A value after gain: clipped to ±1 and rounded to 0.01, so a still
/// picture stays exactly the same and the app sleeps.
fn level(v: f32) -> f32 {
    ((v.clamp(-1.0, 1.0) * 100.0).round() / 100.0) + 0.0
}

/// Taps on a Phase Scope's tempo. A pause longer than this starts a fresh count.
const TAP_PAUSE: Duration = Duration::from_secs(2);
/// The most recent taps that count.
const TAPS_KEPT: usize = 8;

/// The taps so far, on one Meter.
#[derive(Default)]
pub(crate) struct Taps {
    meter: usize,
    times: Vec<Duration>,
}

impl Taps {
    /// A tap on `meter` at `now`. Returns the tempo the taps give (BPM, to
    /// 0.1, not yet clamped) from the second tap on.
    pub fn tap(&mut self, meter: usize, now: Duration) -> Option<f32> {
        let fresh = self.meter != meter
            || self
                .times
                .last()
                .is_none_or(|&last| now.saturating_sub(last) > TAP_PAUSE);
        if fresh {
            self.times.clear();
            self.meter = meter;
        }
        self.times.push(now);
        if self.times.len() > TAPS_KEPT {
            self.times.remove(0);
        }
        let (first, last) = (*self.times.first()?, *self.times.last()?);
        let gaps = self.times.len() - 1;
        if gaps == 0 || last <= first {
            return None;
        }
        let beat = (last - first).as_secs_f64() / gaps as f64;
        Some(((60.0 / beat * 10.0).round() / 10.0) as f32)
    }
}

/// The channel splits a Phase Scope offers.
pub const CHANNEL_VIEWS: [ChannelView; 3] = [
    ChannelView::Mono,
    ChannelView::LeftRight,
    ChannelView::MidSide,
];
