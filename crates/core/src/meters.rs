//! The four Meters: their settings, their analysers and what each puts in the scene.

use std::time::Duration;

use dasmeter_analysis::{
    LoudnessAnalyser, LoudnessSettings, Spectrum, SpectrumAnalyser, SpectrumSettings,
    StereoReadings, StereoView, StereometerAnalyser, StereometerSettings, WaveformAnalyser,
    WaveformColumn, WaveformSettings, note_name,
};

use crate::scene::LoudnessDisplay;

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

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SpectrumMeterSettings {
    pub analysis: SpectrumSettings,
    /// Whether the peak-hold curve is drawn. Default on.
    pub show_peak_hold: bool,
}

impl Default for SpectrumMeterSettings {
    fn default() -> Self {
        SpectrumMeterSettings {
            analysis: SpectrumSettings::default(),
            show_peak_hold: true,
        }
    }
}

/// Which loudness the Loudness Meter's LUFS bar shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum LufsBar {
    #[default]
    ShortTerm,
    Momentary,
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

/// Most points the Stereometer keeps, whatever the persistence and rate.
const MAX_STEREO_POINTS: usize = 16_384;

/// One Meter's settings, which also says which Meter it is.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum MeterSettings {
    Waveform(WaveformMeterSettings),
    Spectrum(SpectrumMeterSettings),
    Loudness(LoudnessMeterSettings),
    Stereometer(StereometerMeterSettings),
}

/// Which of the four Meters a pane shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MeterKind {
    Waveform,
    Spectrum,
    Loudness,
    Stereometer,
}

impl MeterKind {
    pub const ALL: [MeterKind; 4] = [
        MeterKind::Waveform,
        MeterKind::Spectrum,
        MeterKind::Loudness,
        MeterKind::Stereometer,
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
        }
    }

    pub fn kind(&self) -> MeterKind {
        match self {
            MeterSettings::Waveform(_) => MeterKind::Waveform,
            MeterSettings::Spectrum(_) => MeterKind::Spectrum,
            MeterSettings::Loudness(_) => MeterKind::Loudness,
            MeterSettings::Stereometer(_) => MeterKind::Stereometer,
        }
    }

    /// The Meter's name, as menus show it.
    pub fn kind_name(&self) -> &'static str {
        match self {
            MeterSettings::Waveform(_) => "Waveform",
            MeterSettings::Spectrum(_) => "Spectrum",
            MeterSettings::Loudness(_) => "Loudness Meter",
            MeterSettings::Stereometer(_) => "Stereometer",
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
    },
    Spectrum {
        settings: SpectrumMeterSettings,
        /// Levels rounded to 0.1 dB and clamped to the dB range's floor.
        spectrum: Spectrum,
        /// Frequencies at the left and right edges (the top is capped at Nyquist).
        range: (f32, f32),
        cursor: Option<CursorReadout>,
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
}

enum Analyser {
    Waveform(Box<WaveformAnalyser>),
    Spectrum(Box<SpectrumAnalyser>),
    Loudness(Box<LoudnessAnalyser>),
    Stereometer(Box<StereometerAnalyser>),
}

/// A Meter in the app core: settings, analyser (once a sample rate is known) and a cached view.
pub(crate) struct Meter {
    settings: MeterSettings,
    analyser: Option<Analyser>,
    view: Option<MeterView>,
}

impl Meter {
    pub fn new(settings: MeterSettings) -> Meter {
        Meter {
            settings,
            analyser: None,
            view: None,
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
    }

    pub fn set_settings(&mut self, settings: MeterSettings) {
        let sample_rate = self.sample_rate();
        let same_kind = std::mem::discriminant(&settings) == std::mem::discriminant(&self.settings);
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
        })
    }

    pub fn process(&mut self, frames: &[f32]) {
        match &mut self.analyser {
            Some(Analyser::Waveform(a)) => a.process(frames),
            Some(Analyser::Spectrum(a)) => a.process(frames),
            Some(Analyser::Loudness(a)) => a.process(frames),
            Some(Analyser::Stereometer(a)) => a.process(frames),
            None => return,
        }
        self.view = None;
    }

    /// The cursor moved over this Meter (or left it): only the Spectrum shows it.
    pub fn pointer_changed(&mut self) {
        if matches!(self.settings, MeterSettings::Spectrum(_)) {
            self.view = None;
        }
    }

    /// What the Meter shows now, or `None` before a Source starts.
    /// `pointer` is the cursor's position inside the Meter (0–1 each way), if it's over it.
    pub fn view(&mut self, pointer: Option<[f32; 2]>) -> Option<&MeterView> {
        if self.view.is_none() {
            self.view = Some(build_view(self.analyser.as_mut()?, &self.settings, pointer));
        }
        self.view.as_ref()
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
            }
        }
        (Analyser::Spectrum(a), MeterSettings::Spectrum(settings)) => {
            let nyquist = a.sample_rate() as f32 / 2.0;
            let (low, high) = settings.analysis.frequency_range;
            let range = (low, high.min(nyquist));
            let mut spectrum = a.update().clone();
            let floor = spectrum.db_range.0;
            for trace in &mut spectrum.traces {
                for level in trace.levels.iter_mut().chain(&mut trace.peak_hold) {
                    *level = round_to(level.max(floor), 0.1);
                }
            }
            let cursor = pointer.map(|[x, _]| {
                let frequency = range.0 * (range.1 / range.0).powf(x);
                CursorReadout {
                    x,
                    frequency,
                    note: note_name(frequency),
                }
            });
            MeterView::Spectrum {
                settings,
                spectrum,
                range,
                cursor,
            }
        }
        (Analyser::Loudness(a), MeterSettings::Loudness(settings)) => MeterView::Loudness {
            settings,
            display: LoudnessDisplay::new(a.sample_rate(), &a.readings()),
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
        _ => unreachable!("a Meter's analyser always matches its settings"),
    }
}
