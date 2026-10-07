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

use dasmeter_analysis::{ChannelView, Lr4};

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
/// How long a Cycle waits after it ends before it is cut, so the Overlay
/// Source's audio for it has arrived too.
const MARGIN_SECONDS: f64 = 0.03;
/// A DAW song position further than this from where the clock expects it
/// (in quarter notes) is a jump: the clock locks on again.
const JUMP_BEATS: f64 = 0.05;
/// What auto gain scales the loudest point to.
const AUTO_GAIN_PEAK: f32 = 0.9;
/// How slowly auto gain lets the peak fall, as a time constant in seconds.
const AUTO_GAIN_RELEASE: f32 = 3.0;

/// One trace across the Cycle: each column's lowest and highest value, −1 to
/// 1 after gain, rounded to 0.01.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ScopeTrace {
    pub min: Vec<f32>,
    pub max: Vec<f32>,
}

impl ScopeTrace {
    fn silent() -> ScopeTrace {
        ScopeTrace {
            min: vec![0.0; COLUMNS],
            max: vec![0.0; COLUMNS],
        }
    }
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
    /// Per column, whether the two push the same way: 1 together, −1 one
    /// the exact opposite of the other (they cancel), 0 unrelated or silent.
    pub phase: Vec<f32>,
    /// How well the two agree below the cut-off, −1 to 1 to 0.01; `None` in
    /// silence.
    pub correlation: Option<f32>,
    /// The picked Send Plugin isn't there: "Waiting for <name>".
    pub waiting: Option<String>,
    /// What would make the two fit better, best first; none when they do.
    pub fits: Vec<Fit>,
    /// The fits in words, when suggestions are turned on.
    pub advice: Vec<String>,
}

/// Something that would make the Overlay Source fit the main one better
/// below the cut-off.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fit {
    /// Flip the Overlay Source's polarity: the correlation `then`.
    Flip { then: f32 },
    /// Move the Overlay Source `ms` (later if positive, as the Offset
    /// does), flipped too if `flip`: the correlation `then`.
    Move { ms: f32, flip: bool, then: f32 },
    /// Their lows sit at different pitches (Hz), so they drift in and out
    /// of phase and no move helps: tune one to the other.
    Pitch { main: f32, overlay: f32 },
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
    overlay: Option<OverlayCut>,
}

/// The Overlay Source's part of a Cycle, before gain.
#[derive(Clone, Debug, PartialEq)]
struct OverlayCut {
    /// Its mono trace.
    trace: (Vec<f32>, Vec<f32>),
    /// The mono sum of both.
    sum: (Vec<f32>, Vec<f32>),
    /// Per column, whether the two push the same way, −1 to 1.
    phase: Vec<f32>,
    /// How well the two agree below the cut-off; `None` if either is silent there.
    correlation: Option<f32>,
    fits: Vec<Fit>,
}

/// Below this mean square (−80 dBFS) a stretch counts as silent: no
/// shading, no correlation.
const SILENT_POWER: f64 = 1e-8;
/// How long the cut-off filters run before the Cycle, in periods of the
/// cut-off frequency, so they've settled when it starts.
const SETTLE_PERIODS: f32 = 4.0;

/// An Overlay Source's audio and clock, and how its frames line up with the
/// main Source's when neither says its song position.
struct Overlay {
    track: Track,
    /// Main frame minus overlay frame, by arrival, once measured.
    arrival: Option<i64>,
}

/// The Phase Scope's analysis.
pub(crate) struct PhaseScope {
    rate: u32,
    settings: PhaseScopeMeterSettings,
    main: Track,
    overlay: Option<Overlay>,
    /// The Cycle being cut as it plays.
    sweep: Option<Sweep>,
    /// Whole Cycles, newest last.
    history: VecDeque<Cut>,
    /// The peak auto gain fills the Meter to: the loudest shown, or falling
    /// from it slowly, so a fade-out fades rather than being scaled back up.
    auto_peak: f32,
}

/// A Cycle being cut column by column as its audio arrives, so the newest
/// trace is drawn as it plays (like a scope's beam) rather than a whole
/// Cycle at a time.
struct Sweep {
    /// Its number on the grid.
    number: i64,
    /// Where it starts and how long it is, in quarter notes.
    start: f64,
    length: f64,
    /// The next column to cut, and the frame it starts at.
    next: usize,
    from: f64,
    /// The columns cut so far; the rest are zero.
    cut: Cut,
    overlay: Option<OverlayPass>,
}

impl PhaseScope {
    pub fn new(rate: u32, settings: PhaseScopeMeterSettings) -> PhaseScope {
        PhaseScope {
            rate,
            settings,
            main: Track::new(rate),
            overlay: None,
            sweep: None,
            history: VecDeque::new(),
            auto_peak: 0.0,
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
            || old.overlay_offset != settings.overlay_offset
        {
            self.history.clear();
            self.sweep = None;
        }
    }

    /// Starts (afresh) or stops taking an Overlay Source.
    pub fn set_overlay(&mut self, on: bool) {
        self.overlay = on.then(|| Overlay {
            track: Track::new(self.rate),
            arrival: None,
        });
        self.history.clear();
        self.sweep = None;
    }

    /// Takes the Overlay Source's audio, which shows with the next Cycle.
    pub fn process_overlay(&mut self, frames: &[f32], timing: Option<Timing>) {
        let free = f64::from(self.settings.tempo);
        let Some(overlay) = &mut self.overlay else {
            return;
        };
        // Line up by arrival: this block came in about when the main
        // Source's newest did. Measured once, again only after a big slip
        // (a dropout), so the overlay doesn't jitter by a block.
        let now = self.main.end as i64 - overlay.track.end as i64;
        let slip = (self.rate / 10) as i64;
        if overlay.arrival.is_none_or(|a| (a - now).abs() > slip) {
            overlay.arrival = Some(now);
        }
        overlay.track.push(frames, timing, free);
    }

    /// Takes the main Source's audio. Returns whether the picture moved on.
    pub fn process(&mut self, frames: &[f32], timing: Option<Timing>) -> bool {
        if self
            .main
            .push(frames, timing, f64::from(self.settings.tempo))
        {
            // Cycles are counted afresh from where the clock locked on.
            self.sweep = None;
        }
        let swept = self.sweep_on();
        let seconds = (frames.len() / 2) as f32 / self.rate as f32;
        let peak = self.peak();
        let falling = self.auto_peak * (-seconds / AUTO_GAIN_RELEASE).exp();
        self.auto_peak = if peak >= falling { peak } else { falling };
        swept
    }

    /// Cuts the columns whose audio has arrived, Cycle after Cycle. Returns
    /// whether any were cut.
    fn sweep_on(&mut self) -> bool {
        let Some(ready) = self.main.ready_beats() else {
            return false;
        };
        let grid = self.main.grid;
        let length = grid.cycle(self.settings.cycle);
        let mut cut_any = false;
        let mut sweep = match self.sweep.take() {
            Some(sweep) => sweep,
            None => {
                let number = ((ready - grid.origin) / length).floor() as i64;
                match self.begin(number, length) {
                    Some(sweep) => sweep,
                    None => return false,
                }
            }
        };
        loop {
            while sweep.next < COLUMNS {
                let end = sweep.start + sweep.length * (sweep.next + 1) as f64 / COLUMNS as f64;
                if end > ready {
                    self.sweep = Some(sweep);
                    return cut_any;
                }
                if self.cut_column(&mut sweep, end).is_none() {
                    // The clock or the ring no longer covers it: start afresh.
                    return cut_any;
                }
                cut_any = true;
            }
            let Sweep {
                number,
                mut cut,
                overlay,
                ..
            } = sweep;
            cut.overlay = overlay.map(OverlayPass::finish);
            self.history.push_back(cut);
            while self.history.len() > AVERAGED.max(TRAIL + 1) {
                self.history.pop_front();
            }
            match self.begin(number + 1, length) {
                Some(next) => sweep = next,
                None => return cut_any,
            }
        }
    }

    /// Cycle `number` (from the grid's origin), `length` quarter notes
    /// long, ready to cut, if the ring still holds its start since the
    /// clock last locked on.
    fn begin(&self, number: i64, length: f64) -> Option<Sweep> {
        let start = self.main.grid.origin + number as f64 * length;
        let first = self.main.frame_at(start)?;
        if first < self.main.oldest() as f64 {
            return None;
        }
        let count = self.channel_view().traces();
        Some(Sweep {
            number,
            start,
            length,
            next: 0,
            from: first,
            cut: Cut {
                traces: vec![(vec![0.0f32; COLUMNS], vec![0.0f32; COLUMNS]); count],
                overlay: None,
            },
            overlay: self
                .overlay
                .as_ref()
                .map(|o| OverlayPass::new(self, o, start, first)),
        })
    }

    /// Cuts `sweep`'s next column, which ends at song position `end`.
    fn cut_column(&self, sweep: &mut Sweep, end: f64) -> Option<()> {
        let track = &self.main;
        let column = sweep.next;
        let beats = sweep.start + sweep.length * column as f64 / COLUMNS as f64;
        let to = track.frame_at(end)?;
        let from = sweep.from;
        let (a, b) = (
            from.floor() as u64,
            (to.floor() as u64).max(from.floor() as u64 + 1),
        );
        if a < track.oldest() {
            return None;
        }
        let view = self.channel_view();
        let count = sweep.cut.traces.len();
        let (mut low, mut high) = ([f32::MAX; 2], [f32::MIN; 2]);
        for frame in a..b {
            let [l, r] = track.sample(frame);
            let split = view.split(l, r);
            for t in 0..count {
                low[t] = low[t].min(split[t]);
                high[t] = high[t].max(split[t]);
            }
        }
        for (t, (min, max)) in sweep.cut.traces.iter_mut().enumerate() {
            min[column] = low[t];
            max[column] = high[t];
        }
        if let Some(pass) = &mut sweep.overlay {
            let shift = self.overlay_frame(beats, a).map(|o| o - a as i64);
            pass.column(self, column, a..b, shift);
        }
        sweep.from = to;
        sweep.next += 1;
        Some(())
    }

    /// The newest Cycle as drawn: the one being cut up to where it has got,
    /// the last whole one after that.
    fn newest(&self) -> Option<Cut> {
        let last = self.history.back();
        let Some(sweep) = &self.sweep else {
            return last.cloned();
        };
        let mut newest = sweep.cut.clone();
        let reached = sweep.next;
        let (Some(last), true) = (last, reached < COLUMNS) else {
            if let Some(pass) = &sweep.overlay {
                newest.overlay = Some(pass.so_far(reached, None));
            }
            return Some(newest);
        };
        if last.traces.len() == newest.traces.len() {
            for (now, before) in newest.traces.iter_mut().zip(&last.traces) {
                now.0[reached..].copy_from_slice(&before.0[reached..]);
                now.1[reached..].copy_from_slice(&before.1[reached..]);
            }
        }
        if let Some(pass) = &sweep.overlay {
            newest.overlay = Some(pass.so_far(reached, last.overlay.as_ref()));
        }
        Some(newest)
    }

    /// The loudest value auto gain would fit in now: over the newest Cycle
    /// as drawn and the trail behind it.
    fn peak(&self) -> f32 {
        let newest = self.newest();
        peak_of(newest.iter().chain(self.trail()))
    }

    /// The whole Cycles drawn fading behind the newest, oldest first; none
    /// when averaging.
    fn trail(&self) -> Vec<&Cut> {
        match self.settings.steadiness {
            Steadiness::Trail => {
                let skip = self.history.len().saturating_sub(TRAIL);
                self.history.iter().skip(skip).collect()
            }
            Steadiness::Average => Vec::new(),
        }
    }

    /// Why no Cycle can show: one longer than the audio kept.
    fn too_long(&self) -> Option<String> {
        let tempo = self.main.tempo().unwrap_or(f64::from(self.settings.tempo));
        let seconds = self.main.grid.cycle(self.settings.cycle) * 60.0 / tempo;
        (seconds > MAX_SECONDS - MARGIN_SECONDS)
            .then(|| "The Cycle is too long at this tempo: pick a beat".to_owned())
    }

    /// The channel split shown: mono while there's an Overlay Source, which
    /// is compared in mono.
    fn channel_view(&self) -> ChannelView {
        match self.overlay {
            Some(_) => ChannelView::Mono,
            None => self.settings.channel_view,
        }
    }

    /// The overlay frame that lines up with main frame `frame`, at song
    /// position `beats`, after the offset: by song position when both follow
    /// their DAW, else by arrival. `None` if the overlay doesn't reach it.
    fn overlay_frame(&self, beats: f64, frame: u64) -> Option<i64> {
        let overlay = self.overlay.as_ref()?;
        let offset_seconds = f64::from(self.settings.overlay_offset) / 1_000.0;
        if self.main.following && overlay.track.following {
            let tempo = self.main.tempo()?;
            let at = overlay
                .track
                .frame_at(beats - offset_seconds * tempo / 60.0)?;
            return Some(at.floor() as i64);
        }
        let arrival = overlay.arrival?;
        Some(frame as i64 - arrival - (offset_seconds * f64::from(self.rate)).round() as i64)
    }

    /// What the scope shows now.
    pub fn view(&self) -> PhaseScopeView {
        let s = &self.settings;
        let grid = self.main.grid;
        let trail = self.trail();
        let newest = match s.steadiness {
            Steadiness::Trail => self.newest(),
            Steadiness::Average => {
                let cycles: Vec<&Cut> = self.history.iter().collect();
                let skip = cycles.len().saturating_sub(AVERAGED);
                average(&cycles[skip..])
            }
        };
        let count = self.channel_view().traces();
        let newest = newest.unwrap_or_else(|| Cut {
            traces: vec![(vec![0.0; COLUMNS], vec![0.0; COLUMNS]); count],
            overlay: None,
        });
        let gain = if s.auto_gain {
            let peak = match s.steadiness {
                // Averaged, it moves a Cycle at a time anyway: fit it as it is.
                Steadiness::Average => peak_of([&newest]),
                Steadiness::Trail => self.auto_peak.max(self.peak()),
            };
            if peak > 1e-6 {
                AUTO_GAIN_PEAK / peak
            } else {
                1.0
            }
        } else {
            10f32.powf(s.gain / 20.0)
        };
        let one = |(min, max): &(Vec<f32>, Vec<f32>)| ScopeTrace {
            min: min.iter().map(|&v| level(v * gain)).collect(),
            max: max.iter().map(|&v| level(v * gain)).collect(),
        };
        let scale = |cut: &Cut| -> Vec<ScopeTrace> { cut.traces.iter().map(one).collect() };
        let overlay = self.overlay.as_ref().map(|_| match &newest.overlay {
            Some(o) => ScopeOverlay {
                trace: one(&o.trace),
                sum: one(&o.sum),
                phase: o.phase.clone(),
                correlation: o.correlation,
                fits: o.fits.clone(),
                ..ScopeOverlay::default()
            },
            None => ScopeOverlay {
                trace: ScopeTrace::silent(),
                sum: ScopeTrace::silent(),
                phase: vec![0.0; COLUMNS],
                ..ScopeOverlay::default()
            },
        });
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
            overlay,
            note: self.too_long(),
        }
    }
}

/// Cutting the Overlay Source's part of one Cycle, column by column.
struct OverlayPass {
    cut: OverlayCut,
    /// Both Sources low-passed at the cut-off, run on from frame to frame.
    filters: [Lr4; 2],
    /// Σx, Σy, Σxy, Σx², Σy² of the low-passed Sources, over `frames` frames.
    sums: [f64; 5],
    frames: u64,
    /// Per column, Σxy, Σx², Σy² of the low-passed Sources and its frames,
    /// for the phase lane.
    local: Vec<[f64; 4]>,
    /// The phase lane looks at least this many frames around each column:
    /// one period of the cut-off.
    window: f64,
    /// The low-passed Sources, every `every`th frame, for the fits.
    lows: [Vec<f32>; 2],
    every: u64,
    /// Frames until the next one is kept.
    countdown: u64,
    /// Frames a second.
    rate: f64,
}

impl OverlayPass {
    fn new(scope: &PhaseScope, overlay: &Overlay, start: f64, first: f64) -> OverlayPass {
        let cutoff = scope.settings.cutoff;
        let filter = Lr4::new(scope.rate, cutoff, false);
        let empty = || (vec![0.0f32; COLUMNS], vec![0.0f32; COLUMNS]);
        let mut pass = OverlayPass {
            cut: OverlayCut {
                trace: empty(),
                sum: empty(),
                phase: vec![0.0; COLUMNS],
                correlation: None,
                fits: Vec::new(),
            },
            filters: [filter, filter],
            sums: [0.0; 5],
            frames: 0,
            local: vec![[0.0; 4]; COLUMNS],
            window: f64::from(scope.rate) / f64::from(cutoff),
            lows: [Vec::new(), Vec::new()],
            every: ((scope.rate as f32 / (cutoff * FIT_PER_PERIOD)) as u64).max(1),
            countdown: 0,
            rate: f64::from(scope.rate),
        };
        // Settle the filters on the audio just before the Cycle.
        let a = first.floor() as u64;
        let settle = (SETTLE_PERIODS * scope.rate as f32 / cutoff) as u64;
        if let Some(shift) = scope.overlay_frame(start, a).map(|o| o - a as i64) {
            for frame in a.saturating_sub(settle)..a {
                let (m, o) = pass.pair(scope, overlay, frame, shift);
                pass.filters[0].run(m);
                pass.filters[1].run(o);
            }
        }
        pass
    }

    /// The main and overlay mono samples at main frame `frame`.
    fn pair(&self, scope: &PhaseScope, overlay: &Overlay, frame: u64, shift: i64) -> (f32, f32) {
        let mono = |[l, r]: [f32; 2]| (l + r) / 2.0;
        let main = mono(scope.main.sample(frame));
        let other =
            u64::try_from(frame as i64 + shift).map_or(0.0, |f| mono(overlay.track.sample(f)));
        (main, other)
    }

    /// One column, main frames `frames`, the overlay `shift` frames off (or
    /// not there).
    fn column(
        &mut self,
        scope: &PhaseScope,
        column: usize,
        frames: std::ops::Range<u64>,
        shift: Option<i64>,
    ) {
        let Some(overlay) = &scope.overlay else {
            return;
        };
        let (mut over, mut sum) = ([f32::MAX, f32::MIN], [f32::MAX, f32::MIN]);
        for frame in frames {
            // Nothing from the overlay here: the sum is the main alone.
            let (m, o) = shift.map_or_else(
                || (self.pair(scope, overlay, frame, 0).0, 0.0),
                |shift| self.pair(scope, overlay, frame, shift),
            );
            if shift.is_some() {
                over = [over[0].min(o), over[1].max(o)];
            }
            sum = [sum[0].min(m + o), sum[1].max(m + o)];
            let (x, y) = (self.filters[0].run(m), self.filters[1].run(o));
            if self.countdown == 0 {
                self.lows[0].push(x);
                self.lows[1].push(y);
                self.countdown = self.every;
            }
            self.countdown -= 1;
            let (x, y) = (f64::from(x), f64::from(y));
            for (sum, v) in self.sums.iter_mut().zip([x, y, x * y, x * x, y * y]) {
                *sum += v;
            }
            for (sum, v) in self.local[column]
                .iter_mut()
                .zip([x * y, x * x, y * y, 1.0])
            {
                *sum += v;
            }
            self.frames += 1;
        }
        let cut = &mut self.cut;
        if shift.is_some() {
            (cut.trace.0[column], cut.trace.1[column]) = (over[0], over[1]);
        }
        (cut.sum.0[column], cut.sum.1[column]) = (sum[0], sum[1]);
    }

    fn finish(mut self) -> OverlayCut {
        self.cut.correlation = self.correlation();
        for column in 0..COLUMNS {
            self.cut.phase[column] = self.phase(column, COLUMNS);
        }
        if let Some(now) = self.cut.correlation {
            let rate = self.rate / self.every as f64;
            self.cut.fits = fits(&self.lows[0], &self.lows[1], rate, now);
        }
        self.cut
    }

    /// The Cycle so far: the columns before `reached` cut, the rest from
    /// `last`, the whole Cycle before; the correlation is the last whole
    /// Cycle's, or what there is so far before there's one.
    fn so_far(&self, reached: usize, last: Option<&OverlayCut>) -> OverlayCut {
        let mut cut = self.cut.clone();
        // A column's phase needs the columns around it.
        let settled = reached.saturating_sub(self.reach());
        for column in 0..settled {
            cut.phase[column] = self.phase(column, reached);
        }
        if let Some(last) = last {
            let rest = |now: &mut Vec<f32>, before: &Vec<f32>, from: usize| {
                now[from..].copy_from_slice(&before[from..]);
            };
            rest(&mut cut.trace.0, &last.trace.0, reached);
            rest(&mut cut.trace.1, &last.trace.1, reached);
            rest(&mut cut.sum.0, &last.sum.0, reached);
            rest(&mut cut.sum.1, &last.sum.1, reached);
            rest(&mut cut.phase, &last.phase, settled);
            cut.correlation = last.correlation;
            cut.fits = last.fits.clone();
        } else {
            cut.correlation = self.correlation();
        }
        cut
    }

    /// Pearson over the frames so far: about each Source's mean, so a DC
    /// offset or a lopsided kick doesn't count as agreement. `None` if
    /// either is silent below the cut-off.
    fn correlation(&self) -> Option<f32> {
        let [x, y, xy, xx, yy] = self.sums.map(|v| v / self.frames.max(1) as f64);
        let (vx, vy) = (xx - x * x, yy - y * y);
        (vx.min(vy) > SILENT_POWER)
            .then(|| ratio(((xy - x * y) / (vx * vy).sqrt()).clamp(-1.0, 1.0)))
    }

    /// How many columns either side the phase lane looks: one period of
    /// the cut-off.
    fn reach(&self) -> usize {
        let per_column = (self.frames.max(1) as f64 / self.columns_cut().max(1) as f64).max(1.0);
        (self.window / per_column / 2.0).ceil() as usize
    }

    fn columns_cut(&self) -> usize {
        self.local.iter().filter(|l| l[3] > 0.0).count()
    }

    /// The phase lane at `column`, looking only at columns before `end`:
    /// around it, over one period of the cut-off, how the two lows move. 1
    /// where they're the same, −1 where one is the other upside down, 0
    /// where either is silent.
    fn phase(&self, column: usize, end: usize) -> f32 {
        let reach = self.reach();
        let around = &self.local[column.saturating_sub(reach)..(column + reach + 1).min(end)];
        let [xy, xx, yy, n] = around.iter().fold([0.0; 4], |mut total, part| {
            for (t, v) in total.iter_mut().zip(part) {
                *t += v;
            }
            total
        });
        if xx.min(yy) / n.max(1.0) > SILENT_POWER {
            ratio((xy / (xx * yy).sqrt()).clamp(-1.0, 1.0))
        } else {
            0.0
        }
    }
}

/// The fit suggestions keep the lows this many times a period of the cut-off.
const FIT_PER_PERIOD: f32 = 16.0;
/// The farthest a Move suggestion looks, in seconds.
const FIT_REACH: f64 = 0.02;
/// Tracks agreeing this well get no suggestion.
const FIT_FINE: f32 = 0.9;
/// A move or flip is suggested only if it lifts the correlation this much,
/// to at least `FIT_GOOD`.
const FIT_GAIN: f32 = 0.15;
const FIT_GOOD: f32 = 0.8;
/// Lows this far apart (semitones) are told to tune.
const FIT_SEMITONES: f32 = 0.5;
/// The lowest pitch looked for, in Hz.
const FIT_LOWEST_HZ: f64 = 25.0;
/// A pitch counts when its spectrum peaks this many times above its mean.
const FIT_PITCH_PROMINENCE: f64 = 6.0;

/// What would make `overlay` fit `main` (both low-passed, `rate` a second)
/// better, now correlated `now`.
fn fits(main: &[f32], overlay: &[f32], rate: f64, now: f32) -> Vec<Fit> {
    let mut fits = Vec::new();
    if now >= FIT_FINE {
        return fits;
    }
    // How well they'd agree with the overlay `k` steps earlier, for each k.
    let reach = (FIT_REACH * rate).ceil() as i64;
    let shifted: Vec<(i64, f32)> = (-reach..=reach)
        .map(|k| (k, shifted_correlation(main, overlay, k)))
        .collect();
    // The smallest move within a hair of the best, as is and flipped.
    let best = |sign: f32| -> (i64, f32) {
        let top = shifted
            .iter()
            .map(|&(_, r)| sign * r)
            .fold(f32::MIN, f32::max);
        shifted
            .iter()
            .filter(|&&(_, r)| sign * r >= top - 0.02)
            .min_by_key(|&&(k, _)| k.abs())
            .map(|&(k, r)| (k, sign * r))
            .unwrap_or((0, now))
    };
    let flipped_now = -now;
    let (straight, flipped) = (best(1.0), best(-1.0));
    // Flipping only when it's clearly better: on a steady tone a move does
    // as well.
    let (k, then, flip) = if flipped.1 > straight.1 + 0.05 {
        (flipped.0, flipped.1, true)
    } else {
        (straight.0, straight.1, false)
    };
    let then = ratio(f64::from(then));
    if flipped_now >= then - 0.05 && flipped_now >= FIT_GOOD {
        // Flipping alone does it, with nothing to move.
        fits.push(Fit::Flip {
            then: ratio(f64::from(flipped_now)),
        });
    } else if then >= FIT_GOOD && then - now >= FIT_GAIN {
        // The overlay `k` steps earlier lines up: moved by −k.
        let ms = (-(k as f64) / rate * 1_000.0 * 10.0).round() / 10.0;
        fits.push(Fit::Move {
            ms: ms as f32,
            flip,
            then,
        });
    }
    if let (Some(a), Some(b)) = (pitch(main, rate), pitch(overlay, rate))
        && (12.0 * (b / a).log2()).abs() >= FIT_SEMITONES
    {
        fits.push(Fit::Pitch {
            main: a,
            overlay: b,
        });
    }
    fits
}

/// A fit in words, for the main Source `main` and the Overlay Source `other`.
pub(crate) fn advice(fit: &Fit, main: &str, other: &str) -> String {
    match *fit {
        Fit::Flip { then } => format!("Flip {other}'s polarity: {then:+.2}"),
        Fit::Move { ms, flip, then } => {
            let way = if ms > 0.0 { "later" } else { "earlier" };
            let flip = if flip { "Flip and move" } else { "Move" };
            format!(
                "{flip} {other} {:.1} ms {way} (Offset {ms:+.1}): {then:+.2}",
                ms.abs()
            )
        }
        Fit::Pitch {
            main: a,
            overlay: b,
        } => {
            let note = |hz: f32| {
                dasmeter_analysis::note_name(hz).map_or_else(String::new, |n| format!(" ({n})"))
            };
            let semitones = 12.0 * (a / b).log2();
            let way = if semitones > 0.0 { "up" } else { "down" };
            format!(
                "{main} {a:.0} Hz{}, {other} {b:.0} Hz{}: pitch {other} {way} {:.1} semitones",
                note(a),
                note(b),
                semitones.abs()
            )
        }
    }
}

/// Pearson between `main` and `overlay` read `k` steps on, where they overlap.
fn shifted_correlation(main: &[f32], overlay: &[f32], k: i64) -> f32 {
    let n = main.len().min(overlay.len()) as i64;
    let (from, to) = ((-k).max(0), (n - k).min(n));
    if to - from < 2 {
        return 0.0;
    }
    let pairs = (from..to).map(|i| {
        (
            f64::from(main[i as usize]),
            f64::from(overlay[(i + k) as usize]),
        )
    });
    let count = (to - from) as f64;
    let [x, y, xy, xx, yy] = pairs.fold([0.0; 5], |s, (x, y)| {
        [s[0] + x, s[1] + y, s[2] + x * y, s[3] + x * x, s[4] + y * y]
    });
    let (x, y, xy, xx, yy) = (x / count, y / count, xy / count, xx / count, yy / count);
    let (vx, vy) = (xx - x * x, yy - y * y);
    if vx.min(vy) <= SILENT_POWER {
        return 0.0;
    }
    ((xy - x * y) / (vx * vy).sqrt()).clamp(-1.0, 1.0) as f32
}

/// The pitch of a low-passed Source, in Hz, if it has a clear one: where
/// its spectrum peaks, looked at every quarter semitone (Goertzel) and
/// refined between them. A kick's sweep is brief, so its tail wins.
fn pitch(lows: &[f32], rate: f64) -> Option<f32> {
    const STEPS_PER_OCTAVE: f64 = 48.0;
    // Up to twice the cut-off: above it the lows are mostly filtered out.
    let highest = (rate / f64::from(FIT_PER_PERIOD) * 2.0).min(rate / 2.0);
    let steps = (STEPS_PER_OCTAVE * (highest / FIT_LOWEST_HZ).log2()).floor() as usize;
    if steps < 3 || lows.len() < 16 {
        return None;
    }
    let frequency = |step: f64| FIT_LOWEST_HZ * 2f64.powf(step / STEPS_PER_OCTAVE);
    let power: Vec<f64> = (0..=steps)
        .map(|step| {
            let coefficient = 2.0 * (std::f64::consts::TAU * frequency(step as f64) / rate).cos();
            let (mut s1, mut s2) = (0.0, 0.0);
            for &x in lows {
                let s0 = f64::from(x) + coefficient * s1 - s2;
                (s2, s1) = (s1, s0);
            }
            s1 * s1 + s2 * s2 - coefficient * s1 * s2
        })
        .collect();
    let mean = power.iter().sum::<f64>() / power.len() as f64;
    let (i, &top) = power.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1))?;
    // A clear peak, not noise or silence.
    if mean <= 0.0 || top < mean * FIT_PITCH_PROMINENCE || i == 0 || i == steps {
        return None;
    }
    let (a, b, c) = (power[i - 1], top, power[i + 1]);
    let bend = a - 2.0 * b + c;
    let refine = if bend.abs() > 0.0 {
        0.5 * (a - c) / bend
    } else {
        0.0
    };
    Some(frequency(i as f64 + refine.clamp(-0.5, 0.5)) as f32)
}

/// The loudest value in `cuts`, overlay and sum included.
fn peak_of<'a>(cuts: impl IntoIterator<Item = &'a Cut>) -> f32 {
    cuts.into_iter()
        .flat_map(|c| {
            let overlay = c.overlay.iter().flat_map(|o| [&o.trace, &o.sum]);
            c.traces.iter().chain(overlay)
        })
        .flat_map(|(min, max)| min.iter().chain(max))
        .fold(0.0f32, |peak, v| peak.max(v.abs()))
}

/// A ratio rounded to 0.01, so a still picture stays exactly the same.
fn ratio(v: f64) -> f32 {
    ((v * 100.0).round() / 100.0) as f32 + 0.0
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
    let overlays: Option<Vec<&OverlayCut>> = cuts.iter().map(|c| c.overlay.as_ref()).collect();
    let overlay = overlays.map(|o| {
        let mean = |f: &dyn Fn(&OverlayCut, usize) -> f32| -> Vec<f32> {
            (0..COLUMNS)
                .map(|i| o.iter().map(|c| f(c, i)).sum::<f32>() / n)
                .collect()
        };
        let correlations: Vec<f32> = o.iter().filter_map(|c| c.correlation).collect();
        OverlayCut {
            trace: (mean(&|c, i| c.trace.0[i]), mean(&|c, i| c.trace.1[i])),
            sum: (mean(&|c, i| c.sum.0[i]), mean(&|c, i| c.sum.1[i])),
            phase: mean(&|c, i| c.phase[i])
                .into_iter()
                .map(|v| ratio(f64::from(v)))
                .collect(),
            correlation: (!correlations.is_empty()).then(|| {
                ratio(f64::from(correlations.iter().sum::<f32>()) / correlations.len() as f64)
            }),
            fits: o.last().map(|c| c.fits.clone()).unwrap_or_default(),
        }
    });
    Some(Cut { traces, overlay })
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
