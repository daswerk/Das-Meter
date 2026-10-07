//! The Meters: their settings, their analysers and what each puts in the scene.

use std::time::Duration;

use dasmeter_analysis::{
    CepstrumAnalyser, CepstrumSettings, ChannelView, LoudnessAnalyser, LoudnessSettings,
    SpectrogramAnalyser, SpectrogramSettings, Spectrum, SpectrumAnalyser, SpectrumSettings,
    StereoReadings, StereoView, StereometerAnalyser, StereometerSettings, WaveformAnalyser,
    WaveformColumn, WaveformSettings, note_name,
};

use crate::phase_scope::{PhaseScope, PhaseScopeView};
use crate::scene::{Level, LoudnessDisplay};
use crate::sources::Timing;

/// How the Waveform is coloured.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum WaveformColouring {
    /// One envelope, its colour mixed from the low, mid and high band energies. The default.
    #[default]
    Rgb,
    /// The three bands drawn over each other, low widest, as DJ software does.
    Rekordbox,
    /// One envelope in one colour.
    Solid,
}

#[derive(Clone, Copy, Debug, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct WaveformMeterSettings {
    pub analysis: WaveformSettings,
    pub colouring: WaveformColouring,
}

/// How the Spectrum is coloured.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum SpectrumColouring {
    /// The Theme's Spectrum colours. The default.
    #[default]
    Plain,
    /// Harmonics that keep sounding glow brighter, so the dominant ones
    /// (in the bass, say) stand out in the Spectrum itself.
    SteadyHarmonics,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SpectrumMeterSettings {
    pub analysis: SpectrumSettings,
    /// Whether the peak-hold curve is drawn. Default on.
    pub show_peak_hold: bool,
    /// Whether a line marks the loudest peak, with its frequency and note. Default on.
    pub show_peak_line: bool,
    /// Whether a line and readout follow the mouse over the Spectrum.
    /// Default off: the peak line's readout is enough.
    pub show_cursor: bool,
    /// Whether the Spectrum slows down and keeps its peaks while the
    /// pointer is over it, the held peaks marked with their notes. Default on.
    pub slow_on_hover: bool,
    pub colouring: SpectrumColouring,
}

impl Default for SpectrumMeterSettings {
    fn default() -> Self {
        SpectrumMeterSettings {
            analysis: SpectrumSettings::default(),
            show_peak_hold: true,
            show_peak_line: true,
            show_cursor: false,
            slow_on_hover: true,
            colouring: SpectrumColouring::Plain,
        }
    }
}

/// The Spectrum's zoom window: the box dragged over the Spectrum, shown
/// larger and at a finer resolution in a window inside the Meter.
#[derive(Clone, Debug, PartialEq)]
pub struct SpectrumZoom {
    /// The dragged box, as fractions of the plot: left, top, right, bottom.
    pub selection: [f32; 4],
    /// Where the zoom window sits, as fractions of the plot: left, top,
    /// right, bottom.
    pub panel: [f32; 4],
    /// The box's frequencies, left to right.
    pub range: (f32, f32),
    /// The box's levels at its bottom and top are the spectrum's `db_range`.
    /// Levels rounded to 0.1 dB and clamped to that range's floor.
    pub spectrum: Spectrum,
    /// The box's steadiest peaks, marked as on the Spectrum while it's
    /// held; `x` is across the zoom window.
    pub peaks: Vec<CursorReadout>,
}

/// The Spectrogram's zoom window: the frequencies of the box dragged over
/// it, at the finest resolution and scrolling slower, so their detail shows.
#[derive(Clone, Debug, PartialEq)]
pub struct SpectrogramZoom {
    /// The dragged box, as fractions of the picture: left, top, right, bottom.
    pub selection: [f32; 4],
    /// Where the zoom window sits, as fractions of the picture.
    pub panel: [f32; 4],
    /// The box's frequencies, bottom to top.
    pub range: (f32, f32),
    /// As the Spectrogram's own.
    pub columns: Vec<Vec<u8>>,
    pub completed: u64,
    pub lag: f32,
}

/// How many times slower than its Spectrogram the zoom window scrolls.
pub const ZOOM_SLOWER: u32 = 3;
/// The longest the zoom window's picture spans.
pub const ZOOM_MAX_SPAN: Duration = Duration::from_secs(60);

/// How far a Meter's drawing sits inside its frame, in logical px.
pub const METER_INSET: f32 = 10.0;
/// The zoom window's title bar, which holds its close button at the right,
/// in logical px.
pub const ZOOM_TITLE_HEIGHT: f32 = 20.0;

/// The FFT size the zoom window analyses with: the largest, for the finest
/// frequency resolution.
pub const ZOOM_FFT_SIZE: usize = dasmeter_analysis::spectrum::MAX_FFT_SIZE;
/// Points across the zoom window's line.
pub const ZOOM_POINTS: usize = 400;
/// The smallest box that zooms, as fractions of the plot; a smaller drag is
/// a plain click.
pub const MIN_ZOOM_BOX: [f32; 2] = [0.02, 0.04];

/// How many held peaks are marked while the pointer is over a Spectrum.
pub const HELD_PEAKS: usize = 5;
/// How far a marked peak must stand above the average level around it, in
/// dB (on levels averaged over a couple of seconds).
const HELD_PEAK_PROMINENCE_DB: f32 = 9.0;
/// How far either side that average reaches, in octaves.
const HELD_PEAK_SURROUNDINGS_OCTAVES: f32 = 1.0 / 3.0;
/// The quietest a marked peak gets below the peak readout's, in dB.
const HELD_PEAK_RANGE_DB: f32 = 36.0;
/// A marked peak stays where it was found while a peak within this many
/// octaves (a semitone) is still there.
const HELD_PEAK_STICK_OCTAVES: f32 = 1.0 / 12.0;
/// How many updates a marked peak outlasts its peak, so one that dips for
/// a moment isn't replaced.
const HELD_PEAK_KEEP_UPDATES: u32 = 90;
/// The least distance between two marked peaks, in octaves.
const HELD_PEAK_SPACING_OCTAVES: f32 = 1.0 / 6.0;

/// How far behind its newest column a scrolling Meter draws, in seconds:
/// enough to bridge the gaps between audio blocks, so it scrolls the same
/// distance every frame instead of jumping a block at a time.
const PLAYHEAD_LAG: f64 = 0.04;
/// A playhead further than this from where it should be jumps there, in seconds.
const PLAYHEAD_JUMP: f64 = 0.25;
/// The share of its drift a playhead corrects each frame.
const PLAYHEAD_CORRECTION: f64 = 0.08;

/// The zoom window's place for a box: the half of the plot the box's centre
/// isn't in, so the box stays in view.
pub fn zoom_panel(selection: [f32; 4]) -> [f32; 4] {
    let centre = (selection[0] + selection[2]) / 2.0;
    if centre > 0.5 {
        [0.01, 0.03, 0.5, 0.97]
    } else {
        [0.5, 0.03, 0.99, 0.97]
    }
}

/// Which loudness the Loudness Meter's LUFS bar shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum LufsBar {
    #[default]
    ShortTerm,
    Momentary,
}

/// Which reading the Loudness Meter shows large when it shows numbers only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum BigReading {
    #[default]
    ShortTerm,
    Momentary,
    Integrated,
    /// The true-peak maximum since the last reset.
    TruePeak,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct LoudnessMeterSettings {
    pub analysis: LoudnessSettings,
    /// The target line in LUFS, or `None` for no target. Default −14.
    pub target: Option<f64>,
    pub lufs_bar: LufsBar,
    /// The bars' range in dB (dBFS for levels, LUFS for loudness). Default −60 to 0.
    pub bar_range: (f64, f64),
    /// Whether the true-peak maximum is shown. Default on.
    pub show_true_peak: bool,
    /// Whether the loudness range (LRA) is shown. Default on.
    pub show_range: bool,
    /// Whether PLR and PSR (peak to loudness) are shown. Default on.
    pub show_peak_to_loudness: bool,
    /// Whether the loudness graph (the LUFS bar's reading over time) is shown
    /// where there's room. Default on.
    pub show_history: bool,
    /// How much time the loudness graph spans. Default 30 s.
    #[serde(with = "dasmeter_analysis::seconds")]
    pub history_span: Duration,
    /// Whether the L/R and LUFS bars are shown. Default off: the readings as
    /// numbers, one of them large.
    pub show_bars: bool,
    /// The loudness reading shown large while the bars are off, beside the
    /// sample peak. Default short-term.
    pub big_reading: BigReading,
    /// Whether a thin bar stands beside each big number. Default off.
    pub show_thin_bars: bool,
}

impl Default for LoudnessMeterSettings {
    fn default() -> Self {
        LoudnessMeterSettings {
            analysis: LoudnessSettings::default(),
            target: Some(-14.0),
            lufs_bar: LufsBar::ShortTerm,
            bar_range: (-60.0, 0.0),
            show_true_peak: true,
            show_range: true,
            show_peak_to_loudness: true,
            show_history: true,
            history_span: Duration::from_secs(30),
            show_bars: false,
            big_reading: BigReading::ShortTerm,
            show_thin_bars: false,
        }
    }
}

/// How the Stereometer's points are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum StereoDrawing {
    #[default]
    Dots,
    Lines,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct StereometerMeterSettings {
    /// Analysis settings. `points` is derived from `persistence` and the sample rate.
    pub analysis: StereometerSettings,
    pub view: StereoView,
    pub drawing: StereoDrawing,
    /// How long a point stays visible, fading as it ages. Default 50 ms.
    #[serde(with = "dasmeter_analysis::seconds")]
    pub persistence: Duration,
    /// Whether the balance bar is shown. Default off.
    pub show_balance: bool,
    /// Correlation below this turns the bar negative-coloured. Default 0.
    pub correlation_threshold: f32,
}

impl Default for StereometerMeterSettings {
    fn default() -> Self {
        StereometerMeterSettings {
            analysis: StereometerSettings::default(),
            view: StereoView::Polar,
            drawing: StereoDrawing::Dots,
            persistence: Duration::from_millis(50),
            show_balance: false,
            correlation_threshold: 0.0,
        }
    }
}

/// The Cepstrum Meter: the cepstrum of the mono sum and the pitch it finds.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CepstrumMeterSettings {
    pub analysis: CepstrumSettings,
    /// Whether the detected pitch is marked and read out (Hz and note). Default on.
    pub show_pitch: bool,
}

impl Default for CepstrumMeterSettings {
    fn default() -> Self {
        CepstrumMeterSettings {
            analysis: CepstrumSettings::default(),
            show_pitch: true,
        }
    }
}

/// The Spectrogram: the Spectrum of the mono sum over time, scrolling left.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SpectrogramMeterSettings {
    pub analysis: SpectrogramSettings,
    /// Whether the frequency scale is shown on the left. Default on.
    pub show_scale: bool,
}

impl Default for SpectrogramMeterSettings {
    fn default() -> Self {
        SpectrogramMeterSettings {
            analysis: SpectrogramSettings::default(),
            show_scale: true,
        }
    }
}

/// How long a Phase Scope's Cycle is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum CycleLength {
    /// One beat. The default.
    #[default]
    Beat,
    /// One bar, with its beats marked.
    Bar,
}

/// How a Phase Scope steadies its picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Steadiness {
    /// The newest Cycle sharp, the few before it fading behind. The default.
    #[default]
    Trail,
    /// The last few Cycles averaged: one-off notes fade, the pattern stays.
    Average,
}

/// The Phase Scope: the waveform over one Cycle, held still against the beat.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct PhaseScopeMeterSettings {
    pub cycle: CycleLength,
    /// The tempo when no DAW says one (System Capture, an older Send
    /// Plugin), in BPM. Default 120.
    pub tempo: f32,
    pub channel_view: ChannelView,
    /// Whether the trace is filled to the centre line. Default off: a line.
    pub filled: bool,
    /// Whether the loudest point fills the Meter. Default on.
    pub auto_gain: bool,
    /// The gain without auto gain, in dB. Default 0.
    pub gain: f32,
    pub steadiness: Steadiness,
    /// The correlation's cut-off: it compares the two Sources below this, in Hz.
    /// Default 150.
    pub cutoff: f32,
    /// How far the Overlay Source is moved, in ms: later if positive.
    pub overlay_offset: f32,
}

impl Default for PhaseScopeMeterSettings {
    fn default() -> Self {
        PhaseScopeMeterSettings {
            cycle: CycleLength::Beat,
            tempo: 120.0,
            channel_view: ChannelView::Mono,
            filled: false,
            auto_gain: true,
            gain: 0.0,
            steadiness: Steadiness::Trail,
            cutoff: 150.0,
            overlay_offset: 0.0,
        }
    }
}

/// Most points the Stereometer keeps, whatever the persistence and rate.
const MAX_STEREO_POINTS: usize = 16_384;

/// One Meter's settings, which also says which Meter it is.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum MeterSettings {
    Waveform(WaveformMeterSettings),
    Spectrum(SpectrumMeterSettings),
    Loudness(LoudnessMeterSettings),
    Stereometer(StereometerMeterSettings),
    Cepstrum(CepstrumMeterSettings),
    Spectrogram(SpectrogramMeterSettings),
    PhaseScope(PhaseScopeMeterSettings),
}

/// Which kind of Meter a pane shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MeterKind {
    Waveform,
    Spectrum,
    Loudness,
    Stereometer,
    Cepstrum,
    Spectrogram,
    PhaseScope,
}

impl MeterKind {
    pub const ALL: [MeterKind; 7] = [
        MeterKind::Waveform,
        MeterKind::Spectrum,
        MeterKind::Loudness,
        MeterKind::Stereometer,
        MeterKind::Cepstrum,
        MeterKind::Spectrogram,
        MeterKind::PhaseScope,
    ];
}

impl MeterSettings {
    /// A Meter of `kind` on default settings.
    pub fn default_of(kind: MeterKind) -> MeterSettings {
        match kind {
            MeterKind::Waveform => MeterSettings::Waveform(WaveformMeterSettings::default()),
            MeterKind::Spectrum => MeterSettings::Spectrum(SpectrumMeterSettings::default()),
            MeterKind::Loudness => MeterSettings::Loudness(LoudnessMeterSettings::default()),
            MeterKind::Stereometer => {
                MeterSettings::Stereometer(StereometerMeterSettings::default())
            }
            MeterKind::Cepstrum => MeterSettings::Cepstrum(CepstrumMeterSettings::default()),
            MeterKind::Spectrogram => {
                MeterSettings::Spectrogram(SpectrogramMeterSettings::default())
            }
            MeterKind::PhaseScope => MeterSettings::PhaseScope(PhaseScopeMeterSettings::default()),
        }
    }

    pub fn kind(&self) -> MeterKind {
        match self {
            MeterSettings::Waveform(_) => MeterKind::Waveform,
            MeterSettings::Spectrum(_) => MeterKind::Spectrum,
            MeterSettings::Loudness(_) => MeterKind::Loudness,
            MeterSettings::Stereometer(_) => MeterKind::Stereometer,
            MeterSettings::Cepstrum(_) => MeterKind::Cepstrum,
            MeterSettings::Spectrogram(_) => MeterKind::Spectrogram,
            MeterSettings::PhaseScope(_) => MeterKind::PhaseScope,
        }
    }

    /// The Meter's name, as menus show it.
    pub fn kind_name(&self) -> &'static str {
        match self {
            MeterSettings::Waveform(_) => "Waveform",
            MeterSettings::Spectrum(_) => "Spectrum",
            MeterSettings::Loudness(_) => "Loudness Meter",
            MeterSettings::Stereometer(_) => "Stereometer",
            MeterSettings::Cepstrum(_) => "Cepstrum",
            MeterSettings::Spectrogram(_) => "Spectrogram",
            MeterSettings::PhaseScope(_) => "Phase Scope",
        }
    }
}

/// Where the pointer is over a Spectrum: the frequency under it and the nearest note.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CursorReadout {
    /// Horizontal position in the Meter, 0 (left) to 1 (right).
    pub x: f32,
    pub frequency: f32,
    pub note: Option<dasmeter_analysis::Note>,
    /// The level there in dB, to 0.1 dB: set for the Spectrum's loudest peak.
    pub level: Option<f32>,
}

/// What a live Meter shows: its settings and its draw data.
#[derive(Clone, Debug, PartialEq)]
pub enum MeterView {
    Waveform {
        settings: WaveformMeterSettings,
        /// Completed columns per trace, oldest first; the newest sits at the right edge.
        traces: Vec<Vec<WaveformColumn>>,
        /// Columns completed since the start: the newest column's number plus one.
        completed: u64,
        /// How many columns (fractional) the picture is drawn behind its
        /// newest, which are still to scroll in: it moves steadily between
        /// audio blocks. 0 in silence.
        lag: f32,
    },
    Spectrum {
        settings: SpectrumMeterSettings,
        /// Levels rounded to 0.1 dB and clamped to the dB range's floor.
        spectrum: Spectrum,
        /// Frequencies at the left and right edges (the top is capped at Nyquist).
        range: (f32, f32),
        cursor: Option<CursorReadout>,
        /// The loudest peak, when the peak line is on and there's sound.
        peak: Option<CursorReadout>,
        /// The box being dragged, as fractions of the plot: left, top,
        /// right, bottom.
        selecting: Option<[f32; 4]>,
        /// The open zoom window.
        zoom: Option<Box<SpectrumZoom>>,
        /// Where the pointer is over the Meter (0–1 each way across its
        /// frame), if it is: the zoom window shows the pitch under it.
        pointer: Option<[f32; 2]>,
        /// While the pointer is over the Spectrum (and it slows on hover):
        /// the loudest held peaks, with their frequencies, notes and levels.
        held_peaks: Vec<CursorReadout>,
    },
    Loudness {
        settings: LoudnessMeterSettings,
        display: LoudnessDisplay,
    },
    Stereometer {
        settings: StereometerMeterSettings,
        /// Correlation and balance rounded to 0.01.
        readings: StereoReadings,
        /// Points in −1..1, oldest first.
        points: Vec<[f32; 2]>,
    },
    Cepstrum {
        settings: CepstrumMeterSettings,
        /// The cepstrum from the shortest period (left) to the longest, 0–1,
        /// rounded to 0.01.
        values: Vec<f32>,
        /// The periods at the left and right edges, in seconds.
        quefrency_range: (f32, f32),
        /// The detected pitch, when it's on and there is one: where its
        /// period sits (0–1 across), its frequency and its note.
        pitch: Option<CursorReadout>,
    },
    Spectrogram {
        settings: SpectrogramMeterSettings,
        /// Oldest first, the newest at the right edge; each column's levels
        /// (0 for the dB range's floor to 255 for its top) from the lowest
        /// frequency up, at log-spaced frequencies across `range`.
        columns: Vec<Vec<u8>>,
        /// Frequencies at the bottom and top (the top is capped at Nyquist).
        range: (f32, f32),
        /// Columns added since the start; 0 while all of it is silence, so
        /// the scene stops changing and the app sleeps.
        completed: u64,
        /// As the Waveform's: columns still to scroll in.
        lag: f32,
        /// The box being dragged over it, as fractions of the picture.
        selecting: Option<[f32; 4]>,
        zoom: Option<SpectrogramZoom>,
    },
    PhaseScope {
        settings: PhaseScopeMeterSettings,
        scope: Box<PhaseScopeView>,
    },
}

/// Paces columns that arrive in bursts (audio comes a block at a time) onto
/// the frames' clock: a scrolling Meter draws [`PLAYHEAD_LAG`] behind its
/// newest column and moves on at the columns' rate, nudged back whenever
/// it drifts.
#[derive(Default)]
struct Playhead {
    /// Columns shown (fractional), and when.
    shown: Option<(f64, Duration)>,
}

impl Playhead {
    /// How many columns behind the newest to draw at `now`.
    fn lag(&mut self, now: Duration, completed: u64, per_second: f64) -> f32 {
        let newest = completed as f64;
        let target = newest - PLAYHEAD_LAG * per_second;
        let shown = match self.shown {
            Some((shown, at)) => {
                let predicted = shown + now.saturating_sub(at).as_secs_f64() * per_second;
                let drift = target - predicted;
                if drift.abs() > PLAYHEAD_JUMP * per_second {
                    target
                } else {
                    predicted + drift * PLAYHEAD_CORRECTION
                }
            }
            None => target,
        };
        let shown = shown.clamp(0.0, newest);
        self.shown = Some((shown, now));
        // To a hundredth of a column: finer would only redraw for nothing.
        (((newest - shown) * 100.0).round() / 100.0) as f32
    }

    fn reset(&mut self) {
        self.shown = None;
    }
}

enum Analyser {
    Waveform(Box<WaveformAnalyser>),
    Spectrum(Box<SpectrumAnalyser>),
    Loudness(Box<LoudnessAnalyser>),
    Stereometer(Box<StereometerAnalyser>),
    Cepstrum(Box<CepstrumAnalyser>),
    Spectrogram(Box<SpectrogramAnalyser>),
    PhaseScope(Box<PhaseScope>),
}

/// A Meter in the app core: settings, analyser (once a sample rate is known) and a cached view.
pub(crate) struct Meter {
    settings: MeterSettings,
    analyser: Option<Analyser>,
    view: Option<MeterView>,
    /// The Spectrum's box being dragged, as fractions of the plot: where
    /// the drag started and where the pointer is.
    selecting: Option<[f32; 4]>,
    zoom: Option<Zoom>,
    playhead: Playhead,
    /// The Spectrum's marked peaks while the pointer is over it.
    marked: MarkedPeaks,
}

/// Peaks marked over a Spectrum, kept where they were found until their
/// peak is gone for a while, so the marks don't hop about.
#[derive(Default)]
struct MarkedPeaks {
    /// Each with how many updates in a row it's been missing.
    peaks: Vec<(CursorReadout, u32)>,
}

impl MarkedPeaks {
    /// Takes this update's candidates (strongest first), or `None` to clear
    /// the marks, and returns the marks, lowest frequency first.
    fn update(&mut self, candidates: Option<Vec<CursorReadout>>) -> Vec<CursorReadout> {
        let Some(candidates) = candidates else {
            self.peaks.clear();
            return Vec::new();
        };
        let near = |a: f32, b: f32, octaves: f32| (a / b).log2().abs() < octaves;
        let mut seen = vec![false; self.peaks.len()];
        for candidate in candidates {
            let found = self.peaks.iter().position(|(peak, _)| {
                near(peak.frequency, candidate.frequency, HELD_PEAK_STICK_OCTAVES)
            });
            if let Some(i) = found {
                // Where it was found; only its level follows.
                if !seen[i] {
                    seen[i] = true;
                    self.peaks[i].0.level = candidate.level;
                    self.peaks[i].1 = 0;
                }
            } else if self.peaks.len() < HELD_PEAKS
                && self.peaks.iter().all(|(peak, _)| {
                    !near(
                        peak.frequency,
                        candidate.frequency,
                        HELD_PEAK_SPACING_OCTAVES,
                    )
                })
            {
                self.peaks.push((candidate, 0));
                seen.push(true);
            }
        }
        for ((_, missing), seen) in self.peaks.iter_mut().zip(seen) {
            if !seen {
                *missing += 1;
            }
        }
        self.peaks
            .retain(|&(_, missing)| missing <= HELD_PEAK_KEEP_UPDATES);
        let mut marks: Vec<CursorReadout> = self.peaks.iter().map(|&(peak, _)| peak).collect();
        marks.sort_by(|a, b| a.frequency.total_cmp(&b.frequency));
        marks
    }
}

/// An open zoom window: its box and the analyser behind it.
struct Zoom {
    selection: [f32; 4],
    range: (f32, f32),
    db_range: (f32, f32),
    analyser: Option<ZoomAnalyser>,
    marked: MarkedPeaks,
    playhead: Playhead,
}

enum ZoomAnalyser {
    Spectrum(Box<SpectrumAnalyser>),
    Spectrogram(Box<SpectrogramAnalyser>),
}

impl Meter {
    pub fn new(settings: MeterSettings) -> Meter {
        Meter {
            settings,
            analyser: None,
            view: None,
            selecting: None,
            zoom: None,
            playhead: Playhead::default(),
            marked: MarkedPeaks::default(),
        }
    }

    pub fn settings(&self) -> MeterSettings {
        self.settings
    }

    pub fn kind_name(&self) -> &'static str {
        self.settings.kind_name()
    }

    /// Starts over at a sample rate: a new Source, or the output changed.
    pub fn start(&mut self, sample_rate: u32) {
        self.analyser = Some(analyser(sample_rate, &self.settings));
        self.view = None;
        self.start_zoom();
    }

    /// Whether this is a Spectrum or a Spectrogram, which zoom into a dragged box.
    pub fn zooms(&self) -> bool {
        matches!(
            self.settings,
            MeterSettings::Spectrum(_) | MeterSettings::Spectrogram(_)
        )
    }

    /// The open zoom window's place, as fractions of the plot.
    pub fn zoom_panel(&self) -> Option<[f32; 4]> {
        self.zoom.as_ref().map(|z| zoom_panel(z.selection))
    }

    /// Starts or moves the box being dragged over the Spectrum.
    pub fn drag_box(&mut self, from: [f32; 2], to: [f32; 2]) {
        let clamp = |v: f32| v.clamp(0.0, 1.0);
        let selecting = Some([clamp(from[0]), clamp(from[1]), clamp(to[0]), clamp(to[1])]);
        if selecting != self.selecting {
            self.selecting = selecting;
            self.view = None;
        }
    }

    /// The drag ended: zooms into the box if it's big enough. Returns whether it zoomed.
    pub fn end_drag(&mut self) -> bool {
        let Some([x0, y0, x1, y1]) = self.selecting.take() else {
            return false;
        };
        self.view = None;
        let selection = [x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)];
        if selection[2] - selection[0] < MIN_ZOOM_BOX[0]
            || selection[3] - selection[1] < MIN_ZOOM_BOX[1]
        {
            return false;
        }
        let nyquist = self
            .sample_rate()
            .map_or(f32::MAX, |rate| rate as f32 / 2.0);
        let (range, db_range) = match self.settings {
            // Frequency across, level up.
            MeterSettings::Spectrum(settings) => {
                let analysis = settings.analysis;
                let (low, high) = (
                    analysis.frequency_range.0,
                    analysis.frequency_range.1.min(nyquist),
                );
                let frequency = |x: f32| low * (high / low).powf(x);
                let (bottom, top) = analysis.db_range;
                let db = |y: f32| top - y * (top - bottom);
                (
                    (frequency(selection[0]), frequency(selection[2])),
                    (db(selection[3]), db(selection[1])),
                )
            }
            // Time across, frequency up: the box's frequencies, all of time.
            MeterSettings::Spectrogram(settings) => {
                let analysis = settings.analysis;
                let (low, high) = (
                    analysis.frequency_range.0,
                    analysis.frequency_range.1.min(nyquist),
                );
                let frequency = |y: f32| low * (high / low).powf(1.0 - y);
                (
                    (frequency(selection[3]), frequency(selection[1])),
                    analysis.db_range,
                )
            }
            _ => return false,
        };
        self.zoom = Some(Zoom {
            selection,
            range,
            db_range,
            analyser: None,
            marked: MarkedPeaks::default(),
            playhead: Playhead::default(),
        });
        self.start_zoom();
        true
    }

    /// Closes the zoom window. Returns whether one was open.
    pub fn close_zoom(&mut self) -> bool {
        self.view = None;
        self.zoom.take().is_some()
    }

    /// (Re)starts the zoom window's analyser at the Meter's sample rate.
    fn start_zoom(&mut self) {
        let (Some(rate), Some(zoom)) = (self.sample_rate(), &mut self.zoom) else {
            return;
        };
        zoom.playhead.reset();
        zoom.analyser = match self.settings {
            MeterSettings::Spectrum(settings) => {
                Some(ZoomAnalyser::Spectrum(Box::new(SpectrumAnalyser::new(
                    rate,
                    SpectrumSettings {
                        fft_size: ZOOM_FFT_SIZE,
                        frequency_range: zoom.range,
                        db_range: zoom.db_range,
                        style: dasmeter_analysis::SpectrumStyle::Line {
                            points: ZOOM_POINTS,
                        },
                        ..settings.analysis
                    },
                ))))
            }
            MeterSettings::Spectrogram(settings) => Some(ZoomAnalyser::Spectrogram(Box::new(
                SpectrogramAnalyser::new(
                    rate,
                    SpectrogramSettings {
                        fft_size: ZOOM_FFT_SIZE,
                        frequency_range: zoom.range,
                        span: (settings.analysis.span * ZOOM_SLOWER).min(ZOOM_MAX_SPAN),
                        ..settings.analysis
                    },
                ),
            ))),
            _ => None,
        };
    }

    /// The Source says it's mono (or not). Only the Stereometer shows it.
    pub fn set_mono(&mut self, mono: bool) {
        if let Some(Analyser::Stereometer(a)) = &mut self.analyser {
            if a.readings().mono != mono {
                a.set_mono(mono);
                self.view = None;
            }
        }
    }

    /// Clears a Loudness Meter's integrated LUFS, LRA and maxima. Other Meters
    /// have nothing to reset; returns whether this one did.
    pub fn reset(&mut self) -> bool {
        let Some(Analyser::Loudness(a)) = &mut self.analyser else {
            return false;
        };
        a.reset();
        self.view = None;
        true
    }

    pub fn stop(&mut self) {
        self.analyser = None;
        self.view = None;
        if let Some(zoom) = &mut self.zoom {
            zoom.analyser = None;
        }
    }

    pub fn set_settings(&mut self, settings: MeterSettings) {
        let sample_rate = self.sample_rate();
        let same_kind = std::mem::discriminant(&settings) == std::mem::discriminant(&self.settings);
        // A new frequency or level range would move the zoomed box: close it.
        let same_analysis = match (self.settings, settings) {
            (MeterSettings::Spectrum(a), MeterSettings::Spectrum(b)) => a.analysis == b.analysis,
            (MeterSettings::Spectrogram(a), MeterSettings::Spectrogram(b)) => {
                a.analysis == b.analysis
            }
            _ => same_kind,
        };
        if !same_analysis {
            self.zoom = None;
            self.selecting = None;
        }
        self.settings = settings;
        self.view = None;
        let Some(sample_rate) = sample_rate else {
            return;
        };
        match (&mut self.analyser, settings) {
            (Some(Analyser::Waveform(a)), MeterSettings::Waveform(s)) => a.set_settings(s.analysis),
            (Some(Analyser::Spectrum(a)), MeterSettings::Spectrum(s)) => {
                if *a.settings() != s.analysis {
                    a.set_settings(s.analysis);
                }
            }
            (Some(Analyser::Loudness(a)), MeterSettings::Loudness(s)) => a.set_settings(s.analysis),
            (Some(Analyser::Stereometer(a)), MeterSettings::Stereometer(s)) => {
                a.set_settings(stereo_analysis(sample_rate, &s))
            }
            (Some(Analyser::Cepstrum(a)), MeterSettings::Cepstrum(s)) => a.set_settings(s.analysis),
            (Some(Analyser::Spectrogram(a)), MeterSettings::Spectrogram(s)) => {
                a.set_settings(s.analysis)
            }
            (Some(Analyser::PhaseScope(a)), MeterSettings::PhaseScope(s)) => a.set_settings(s),
            _ => debug_assert!(!same_kind),
        }
        if !same_kind {
            self.start(sample_rate);
        }
    }

    pub fn sample_rate(&self) -> Option<u32> {
        Some(match self.analyser.as_ref()? {
            Analyser::Waveform(a) => a.sample_rate(),
            Analyser::Spectrum(a) => a.sample_rate(),
            Analyser::Loudness(a) => a.sample_rate(),
            Analyser::Stereometer(a) => a.sample_rate(),
            Analyser::Cepstrum(a) => a.sample_rate(),
            Analyser::Spectrogram(a) => a.sample_rate(),
            Analyser::PhaseScope(a) => a.sample_rate(),
        })
    }

    pub fn process(&mut self, frames: &[f32]) {
        self.process_timed(frames, None);
    }

    /// Takes a block of the Source's audio, with where its DAW was at the
    /// block's first frame if it says.
    pub fn process_timed(&mut self, frames: &[f32], timing: Option<Timing>) {
        match &mut self.analyser {
            Some(Analyser::PhaseScope(a)) => {
                // It only looks different once a Cycle is added.
                if !a.process(frames, timing) {
                    return;
                }
            }
            Some(Analyser::Waveform(a)) => a.process(frames),
            Some(Analyser::Spectrum(a)) => {
                a.process(frames);
                if let Some(ZoomAnalyser::Spectrum(zoom)) =
                    self.zoom.as_mut().and_then(|z| z.analyser.as_mut())
                {
                    zoom.process(frames);
                }
            }
            Some(Analyser::Loudness(a)) => a.process(frames),
            Some(Analyser::Stereometer(a)) => a.process(frames),
            Some(Analyser::Cepstrum(a)) => a.process(frames),
            Some(Analyser::Spectrogram(a)) => {
                let before = a.completed();
                a.process(frames);
                let mut added = a.completed() != before;
                if let Some(ZoomAnalyser::Spectrogram(zoom)) =
                    self.zoom.as_mut().and_then(|z| z.analyser.as_mut())
                {
                    let before = zoom.completed();
                    zoom.process(frames);
                    added |= zoom.completed() != before;
                }
                // It only looks different once a column is added.
                if !added {
                    return;
                }
            }
            None => return,
        }
        self.view = None;
    }

    pub fn is_phase_scope(&self) -> bool {
        matches!(self.settings, MeterSettings::PhaseScope(_))
    }

    /// A Phase Scope starts (afresh) or stops taking an Overlay Source.
    /// Other Meters have none.
    pub fn set_overlay(&mut self, on: bool) {
        if let Some(Analyser::PhaseScope(a)) = &mut self.analyser {
            a.set_overlay(on);
            self.view = None;
        }
    }

    /// The Overlay Source's audio, for a Phase Scope.
    pub fn process_overlay(&mut self, frames: &[f32], timing: Option<Timing>) {
        if let Some(Analyser::PhaseScope(a)) = &mut self.analyser {
            a.process_overlay(frames, timing);
        }
    }

    /// The cursor moved over this Meter (or left it): only the Spectrum shows it.
    pub fn pointer_changed(&mut self) {
        if matches!(self.settings, MeterSettings::Spectrum(_)) {
            self.view = None;
        }
    }

    /// What the Meter shows now, or `None` before a Source starts.
    /// `pointer` is the cursor's position inside the Meter (0–1 each way), if it's over it.
    /// `now` paces the scrolling Meters.
    pub fn view(&mut self, pointer: Option<[f32; 2]>, now: Duration) -> Option<&MeterView> {
        if self.view.is_none() {
            let mut view = build_view(self.analyser.as_mut()?, &self.settings, pointer);
            if let MeterView::Spectrum {
                selecting,
                zoom,
                settings,
                held_peaks,
                ..
            } = &mut view
            {
                *selecting = self.selecting;
                let slow = pointer.is_some() && settings.slow_on_hover;
                *held_peaks = self.marked.update(slow.then(|| std::mem::take(held_peaks)));
                *zoom = self.zoom.as_mut().and_then(|z| z.view(slow)).map(Box::new);
            }
            if let MeterView::Spectrogram {
                selecting, zoom, ..
            } = &mut view
            {
                *selecting = self.selecting;
                *zoom = self.zoom.as_ref().and_then(Zoom::spectrogram_view);
            }
            self.view = Some(view);
        }
        let per_second = match self.analyser.as_ref()? {
            Analyser::Waveform(_) => f64::from(dasmeter_analysis::waveform::COLUMNS_PER_SECOND),
            Analyser::Spectrogram(a) => a.columns_per_second(),
            _ => return self.view.as_ref(),
        };
        if let Some(
            MeterView::Waveform { completed, lag, .. }
            | MeterView::Spectrogram { completed, lag, .. },
        ) = &mut self.view
        {
            *lag = if *completed == 0 {
                self.playhead.reset();
                0.0
            } else {
                self.playhead.lag(now, *completed, per_second)
            };
        }
        if let (
            Some(MeterView::Spectrogram {
                zoom: Some(view), ..
            }),
            Some(zoom),
        ) = (&mut self.view, &mut self.zoom)
        {
            if let Some(ZoomAnalyser::Spectrogram(a)) = &zoom.analyser {
                view.lag = if view.completed == 0 {
                    zoom.playhead.reset();
                    0.0
                } else {
                    zoom.playhead
                        .lag(now, view.completed, a.columns_per_second())
                };
            }
        }
        self.view.as_ref()
    }
}

impl Zoom {
    fn spectrogram_view(&self) -> Option<SpectrogramZoom> {
        let Some(ZoomAnalyser::Spectrogram(a)) = &self.analyser else {
            return None;
        };
        let columns: Vec<Vec<u8>> = a.columns().cloned().collect();
        let silent = columns.iter().flatten().all(|&level| level == 0);
        Some(SpectrogramZoom {
            selection: self.selection,
            panel: zoom_panel(self.selection),
            range: self.range,
            columns,
            completed: if silent { 0 } else { a.completed() },
            lag: 0.0,
        })
    }

    fn view(&mut self, slow: bool) -> Option<SpectrumZoom> {
        let Some(ZoomAnalyser::Spectrum(analyser)) = self.analyser.as_mut() else {
            return None;
        };
        analyser.set_slow(slow);
        let mut spectrum = analyser.update().clone();
        let floor = spectrum.db_range.0;
        for trace in &mut spectrum.traces {
            for level in trace.levels.iter_mut().chain(&mut trace.peak_hold) {
                *level = round_to(level.max(floor), 0.1);
            }
        }
        // Its peaks are always marked: the zoom window is for looking closely.
        let candidates = steady_peaks(analyser, &spectrum, self.range);
        let peaks = self.marked.update(Some(candidates));
        Some(SpectrumZoom {
            selection: self.selection,
            panel: zoom_panel(self.selection),
            range: self.range,
            spectrum,
            peaks,
        })
    }
}

fn stereo_analysis(sample_rate: u32, settings: &StereometerMeterSettings) -> StereometerSettings {
    let points = (settings.persistence.as_secs_f64() * f64::from(sample_rate)).round() as usize;
    StereometerSettings {
        points: points.clamp(1, MAX_STEREO_POINTS),
        ..settings.analysis
    }
}

fn analyser(sample_rate: u32, settings: &MeterSettings) -> Analyser {
    match settings {
        MeterSettings::Waveform(s) => {
            Analyser::Waveform(Box::new(WaveformAnalyser::new(sample_rate, s.analysis)))
        }
        MeterSettings::Spectrum(s) => {
            Analyser::Spectrum(Box::new(SpectrumAnalyser::new(sample_rate, s.analysis)))
        }
        MeterSettings::Loudness(s) => {
            Analyser::Loudness(Box::new(LoudnessAnalyser::new(sample_rate, s.analysis)))
        }
        MeterSettings::Stereometer(s) => Analyser::Stereometer(Box::new(StereometerAnalyser::new(
            sample_rate,
            stereo_analysis(sample_rate, s),
        ))),
        MeterSettings::Cepstrum(s) => {
            Analyser::Cepstrum(Box::new(CepstrumAnalyser::new(sample_rate, s.analysis)))
        }
        MeterSettings::Spectrogram(s) => {
            Analyser::Spectrogram(Box::new(SpectrogramAnalyser::new(sample_rate, s.analysis)))
        }
        MeterSettings::PhaseScope(s) => {
            Analyser::PhaseScope(Box::new(PhaseScope::new(sample_rate, *s)))
        }
    }
}

fn round_to(value: f32, step: f32) -> f32 {
    (value / step).round() * step
}

fn build_view(
    analyser: &mut Analyser,
    settings: &MeterSettings,
    pointer: Option<[f32; 2]>,
) -> MeterView {
    match (analyser, *settings) {
        (Analyser::Waveform(a), MeterSettings::Waveform(settings)) => {
            let traces: Vec<Vec<WaveformColumn>> = (0..a.traces())
                .map(|trace| a.columns(trace).copied().collect())
                .collect();
            // In silence (a flat envelope) every column looks the same however
            // they're grouped: leave the count out, so the scene stops changing
            // and the app sleeps.
            let silent = traces
                .iter()
                .flatten()
                .all(|c| c.min == 0.0 && c.max == 0.0);
            MeterView::Waveform {
                settings,
                traces,
                completed: if silent { 0 } else { a.completed() },
                lag: 0.0,
            }
        }
        (Analyser::Spectrum(a), MeterSettings::Spectrum(settings)) => {
            let nyquist = a.sample_rate() as f32 / 2.0;
            let (low, high) = settings.analysis.frequency_range;
            let range = (low, high.min(nyquist));
            let slow = pointer.is_some() && settings.slow_on_hover;
            a.set_slow(slow);
            let mut spectrum = a.update().clone();
            let floor = spectrum.db_range.0;
            let steady = settings.colouring == SpectrumColouring::SteadyHarmonics;
            for trace in &mut spectrum.traces {
                for level in trace.levels.iter_mut().chain(&mut trace.peak_hold) {
                    *level = round_to(level.max(floor), 0.1);
                }
                if steady {
                    for value in &mut trace.steadiness {
                        *value = round_to(*value, 0.02);
                    }
                } else {
                    trace.steadiness = Vec::new();
                }
            }
            let held_peaks = if slow {
                steady_peaks(a, &spectrum, range)
            } else {
                Vec::new()
            };
            let cursor = pointer.filter(|_| settings.show_cursor).map(|[x, _]| {
                let frequency = range.0 * (range.1 / range.0).powf(x);
                CursorReadout {
                    x,
                    frequency,
                    note: note_name(frequency),
                    level: None,
                }
            });
            let peak = settings
                .show_peak_line
                .then(|| a.steady_peak())
                .flatten()
                .map(|(frequency, level)| CursorReadout {
                    x: ((frequency / range.0).ln() / (range.1 / range.0).ln()).clamp(0.0, 1.0),
                    // To 0.1 Hz and 0.1 dB: finer would only redraw for nothing.
                    frequency: round_to(frequency, 0.1),
                    note: note_name(frequency),
                    level: Some(round_to(level, 0.1)),
                });
            MeterView::Spectrum {
                settings,
                spectrum,
                range,
                cursor,
                peak,
                selecting: None,
                zoom: None,
                pointer,
                held_peaks,
            }
        }
        (Analyser::Loudness(a), MeterSettings::Loudness(settings)) => MeterView::Loudness {
            settings,
            display: {
                let mut display = LoudnessDisplay::new(a.sample_rate(), &a.readings());
                if settings.show_history {
                    display.history = history(a, &settings);
                }
                display
            },
        },
        (Analyser::Stereometer(a), MeterSettings::Stereometer(settings)) => {
            let mut readings = a.readings();
            readings.correlation = round_to(readings.correlation, 0.01);
            readings.balance = round_to(readings.balance, 0.01);
            let mut points = Vec::new();
            a.points(settings.view, &mut points);
            MeterView::Stereometer {
                settings,
                readings,
                points,
            }
        }
        (Analyser::Cepstrum(a), MeterSettings::Cepstrum(settings)) => {
            let cepstrum = a.update();
            let (short, long) = cepstrum.quefrency_range;
            let pitch = settings
                .show_pitch
                .then_some(cepstrum.pitch)
                .flatten()
                .map(|pitch| CursorReadout {
                    x: ((1.0 / pitch.frequency - short) / (long - short)).clamp(0.0, 1.0),
                    // To 0.1 Hz: finer would only redraw for nothing.
                    frequency: round_to(pitch.frequency, 0.1),
                    note: note_name(pitch.frequency),
                    level: None,
                });
            MeterView::Cepstrum {
                settings,
                values: cepstrum.values.iter().map(|&v| round_to(v, 0.01)).collect(),
                quefrency_range: cepstrum.quefrency_range,
                pitch,
            }
        }
        (Analyser::Spectrogram(a), MeterSettings::Spectrogram(settings)) => {
            let nyquist = a.sample_rate() as f32 / 2.0;
            let (low, high) = settings.analysis.frequency_range;
            let columns: Vec<Vec<u8>> = a.columns().cloned().collect();
            let silent = columns.iter().flatten().all(|&level| level == 0);
            MeterView::Spectrogram {
                settings,
                columns,
                range: (low, high.min(nyquist)),
                completed: if silent { 0 } else { a.completed() },
                lag: 0.0,
                selecting: None,
                zoom: None,
            }
        }
        (Analyser::PhaseScope(a), MeterSettings::PhaseScope(settings)) => MeterView::PhaseScope {
            settings,
            scope: Box::new(a.view()),
        },
        _ => unreachable!("a Meter's analyser always matches its settings"),
    }
}

/// Candidates for the marked peaks, strongest first: local maxima of each
/// point's level averaged over a couple of seconds (the loudest trace), so
/// only peaks that keep sounding count, like the steady harmonics. Each
/// stands [`HELD_PEAK_PROMINENCE_DB`] above the average around it and within
/// [`HELD_PEAK_RANGE_DB`] of the peak readout's level, at least
/// [`HELD_PEAK_SPACING_OCTAVES`] from a stronger one. Levels are the held
/// curve's, where the dots sit.
fn steady_peaks(
    a: &SpectrumAnalyser,
    spectrum: &Spectrum,
    range: (f32, f32),
) -> Vec<CursorReadout> {
    let floor = spectrum.db_range.0;
    let loudest = |levels: Vec<&[f32]>| -> Vec<f32> {
        (0..spectrum.frequencies.len())
            .map(|i| {
                levels
                    .iter()
                    .filter_map(|l| l.get(i).copied())
                    .fold(floor, f32::max)
            })
            .collect()
    };
    let steady = loudest(
        (0..spectrum.traces.len())
            .map(|t| a.steady_levels(t))
            .collect(),
    );
    let held = loudest(
        spectrum
            .traces
            .iter()
            .map(|t| t.peak_hold.as_slice())
            .collect(),
    );
    let top = a
        .steady_peak()
        .map(|(_, level)| level)
        .unwrap_or_else(|| steady.iter().copied().fold(floor, f32::max));
    let quietest = (top - HELD_PEAK_RANGE_DB).max(floor);
    // Local maxima standing well above the average around them: noise,
    // averaged, doesn't; a tone or a harmonic that keeps sounding does.
    let frequencies = &spectrum.frequencies;
    let mut candidates: Vec<(usize, f32)> = Vec::new();
    for i in 1..steady.len().saturating_sub(1) {
        let level = steady[i];
        if level <= quietest || level < steady[i - 1] || level < steady[i + 1] {
            continue;
        }
        let (sum, count) = frequencies
            .iter()
            .zip(&steady)
            .filter(|&(&f, _)| (f / frequencies[i]).log2().abs() <= HELD_PEAK_SURROUNDINGS_OCTAVES)
            .fold((0.0, 0), |(sum, count), (_, &l)| (sum + l, count + 1));
        let around = sum / count.max(1) as f32;
        if level - around >= HELD_PEAK_PROMINENCE_DB {
            candidates.push((i, level));
        }
    }
    candidates.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut chosen: Vec<usize> = Vec::new();
    for (i, _) in candidates {
        let f = spectrum.frequencies[i];
        if chosen
            .iter()
            .all(|&j| (f / spectrum.frequencies[j]).log2().abs() >= HELD_PEAK_SPACING_OCTAVES)
        {
            chosen.push(i);
        }
    }
    chosen
        .into_iter()
        .map(|i| {
            let frequency = a.refine(spectrum.frequencies[i]);
            CursorReadout {
                x: ((frequency / range.0).ln() / (range.1 / range.0).ln()).clamp(0.0, 1.0),
                frequency: round_to(frequency, 0.1),
                note: note_name(frequency),
                level: Some(round_to(held[i], 0.1)),
            }
        })
        .collect()
}

/// The loudness graph's points: the LUFS bar's reading every 100 ms over the
/// span, oldest first.
fn history(a: &LoudnessAnalyser, settings: &LoudnessMeterSettings) -> Vec<Level> {
    let steps = (settings.history_span.as_secs_f64() * 10.0).round() as usize;
    let skip = a.history().len().saturating_sub(steps);
    a.history()
        .skip(skip)
        .map(|(momentary, short_term)| {
            let lufs = match settings.lufs_bar {
                LufsBar::ShortTerm => short_term,
                LufsBar::Momentary => momentary,
            };
            Level::from_db(lufs, crate::scene::LUFS_FLOOR)
        })
        .collect()
}
