//! App settings (not per Meter), and the ranges every setting is kept within.
//!
//! The menus and the settings panel send whole settings values; the core keeps
//! them within the ranges the spec gives, so a slider dragged too far, or a
//! hand-edited file later, can't put a Meter into a state it can't draw.

use std::time::Duration;

use dasmeter_analysis::spectrum::{MAX_FFT_SIZE, MIN_FFT_SIZE};
use dasmeter_analysis::{PeakHold, SpectrumStyle, StereoScaling};

use crate::meters::{
    LoudnessMeterSettings, MeterSettings, SpectrumMeterSettings, StereometerMeterSettings,
    WaveformMeterSettings,
};

/// Where the docs page on what the Loudness Meter measures lives.
pub const MEASUREMENTS_URL: &str =
    "https://github.com/daswerk/Das-Meter/blob/main/docs/measurements.md";
/// The line the Loudness Meter's settings show above that link.
pub const MEASUREMENTS_NOTE: &str = "Measures to ITU-R BS.1770-5 and EBU Tech 3341/3342.";
/// Where Help goes.
pub const HELP_URL: &str = crate::docs::SITE;

/// The default frame-rate cap.
pub const DEFAULT_FRAME_RATE_CAP: u32 = 60;
/// The lowest frame-rate cap the settings offer.
pub const MIN_FRAME_RATE_CAP: u32 = 30;

/// Settings that belong to the app, not to a Meter or a Preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// Most frames drawn per second. Default 60, at most the display's refresh rate.
    pub frame_rate_cap: u32,
    /// Look for a new version once a day. Default on.
    pub check_for_updates: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            frame_rate_cap: DEFAULT_FRAME_RATE_CAP,
            check_for_updates: true,
        }
    }
}

impl AppSettings {
    /// The shortest time between two draws.
    pub fn frame_interval(&self) -> Duration {
        Duration::from_nanos(1_000_000_000 / u64::from(self.frame_rate_cap.max(1)))
    }
}

/// The Waveform's time span.
pub const WAVEFORM_SPAN: (Duration, Duration) = (Duration::from_secs(1), Duration::from_secs(30));
/// The Waveform's display gain.
pub const WAVEFORM_GAIN: (f32, f32) = (0.25, 16.0);
/// The Waveform's low/mid crossover, and its mid/high crossover, in Hz.
pub const WAVEFORM_LOW_CROSSOVER: (f32, f32) = (40.0, 1_000.0);
pub const WAVEFORM_HIGH_CROSSOVER: (f32, f32) = (500.0, 12_000.0);
/// The Spectrum's slopes, in dB per octave.
pub const SPECTRUM_SLOPES: [f32; 4] = [0.0, 3.0, 4.5, 6.0];
/// Bands per octave the Spectrum's bars can have.
pub const SPECTRUM_BANDS: [u32; 3] = [3, 6, 12];
/// The Spectrum's bar width when the style switches to bars.
pub const DEFAULT_SPECTRUM_BANDS: u32 = 6;
/// Points of the Spectrum's line when the style switches to a line.
pub const SPECTRUM_LINE_POINTS: usize = 512;
/// The Spectrum's shown frequencies, in Hz.
pub const SPECTRUM_FREQUENCIES: (f32, f32) = (20.0, 20_000.0);
/// The Spectrum's shown levels, in dB.
pub const SPECTRUM_DB: (f32, f32) = (-140.0, 20.0);
/// The Spectrum's attack and release.
pub const SPECTRUM_BALLISTICS: (Duration, Duration) = (Duration::ZERO, Duration::from_secs(5));
/// The Spectrum's widest frequency smoothing, in octaves.
pub const SPECTRUM_MAX_SMOOTHING: f32 = 1.0 / 3.0;
/// Peak hold times, for both the Spectrum and the Loudness Meter.
pub const PEAK_HOLD: (Duration, Duration) = (Duration::from_millis(100), Duration::from_secs(30));
/// The Loudness Meter's target, in LUFS.
pub const LOUDNESS_TARGET: (f64, f64) = (-40.0, 0.0);
/// The Loudness Meter's RMS window.
pub const RMS_WINDOW: (Duration, Duration) = (Duration::from_millis(50), Duration::from_secs(3));
/// The Loudness Meter's bar range, in dB.
pub const LOUDNESS_BAR: (f64, f64) = (-120.0, 6.0);
/// The Stereometer's persistence.
pub const STEREO_PERSISTENCE: (Duration, Duration) =
    (Duration::from_millis(5), Duration::from_millis(340));
/// The Stereometer's fixed gain.
pub const STEREO_GAIN: (f32, f32) = (0.25, 16.0);
/// The Stereometer's correlation averaging time.
pub const CORRELATION_TIME: (Duration, Duration) =
    (Duration::from_millis(50), Duration::from_secs(3));
/// The narrowest a range (dB or Hz ratio) may get, so a Meter never divides by nothing.
const MIN_DB_SPAN: f32 = 6.0;

fn clamp_duration(value: Duration, (low, high): (Duration, Duration)) -> Duration {
    value.clamp(low, high)
}

fn clamp_f32(value: f32, (low, high): (f32, f32), default: f32) -> f32 {
    if value.is_nan() {
        default
    } else {
        value.clamp(low, high)
    }
}

fn clamp_f64(value: f64, (low, high): (f64, f64), default: f64) -> f64 {
    if value.is_nan() {
        default
    } else {
        value.clamp(low, high)
    }
}

fn clamp_hold(hold: PeakHold) -> PeakHold {
    match hold {
        PeakHold::For(time) => PeakHold::For(clamp_duration(time, PEAK_HOLD)),
        PeakHold::Infinite => PeakHold::Infinite,
    }
}

/// The nearest of `options` to `value`.
fn nearest<T: Copy>(value: f32, options: &[T], as_f32: impl Fn(T) -> f32) -> T {
    let mut best = options[0];
    for &option in options {
        if (as_f32(option) - value).abs() < (as_f32(best) - value).abs() {
            best = option;
        }
    }
    best
}

impl MeterSettings {
    /// These settings, kept within the ranges each Meter allows.
    pub fn clamped(self) -> MeterSettings {
        match self {
            MeterSettings::Waveform(s) => MeterSettings::Waveform(s.clamped()),
            MeterSettings::Spectrum(s) => MeterSettings::Spectrum(s.clamped()),
            MeterSettings::Loudness(s) => MeterSettings::Loudness(s.clamped()),
            MeterSettings::Stereometer(s) => MeterSettings::Stereometer(s.clamped()),
        }
    }
}

impl WaveformMeterSettings {
    fn clamped(mut self) -> Self {
        let defaults = WaveformMeterSettings::default().analysis;
        let a = &mut self.analysis;
        a.span = clamp_duration(a.span, WAVEFORM_SPAN);
        a.gain = clamp_f32(a.gain, WAVEFORM_GAIN, defaults.gain);
        a.low_crossover = clamp_f32(
            a.low_crossover,
            WAVEFORM_LOW_CROSSOVER,
            defaults.low_crossover,
        );
        a.high_crossover = clamp_f32(
            a.high_crossover,
            WAVEFORM_HIGH_CROSSOVER,
            defaults.high_crossover,
        );
        // The mid band keeps at least an octave.
        a.high_crossover = a.high_crossover.max(a.low_crossover * 2.0);
        self
    }
}

impl SpectrumMeterSettings {
    fn clamped(mut self) -> Self {
        let defaults = SpectrumMeterSettings::default().analysis;
        let a = &mut self.analysis;
        a.slope = nearest(a.slope, &SPECTRUM_SLOPES, |s| s);
        a.fft_size = a
            .fft_size
            .clamp(MIN_FFT_SIZE, MAX_FFT_SIZE)
            .next_power_of_two()
            .min(MAX_FFT_SIZE);
        let (low, high) = a.frequency_range;
        let low = clamp_f32(low, SPECTRUM_FREQUENCIES, defaults.frequency_range.0);
        let high = clamp_f32(high, SPECTRUM_FREQUENCIES, defaults.frequency_range.1);
        // At least an octave shown.
        a.frequency_range = if high >= low * 2.0 {
            (low, high)
        } else {
            (
                low.min(SPECTRUM_FREQUENCIES.1 / 2.0),
                (low * 2.0).min(SPECTRUM_FREQUENCIES.1),
            )
        };
        let (floor, top) = a.db_range;
        let floor = clamp_f32(floor, SPECTRUM_DB, defaults.db_range.0);
        let top = clamp_f32(top, SPECTRUM_DB, defaults.db_range.1);
        a.db_range = (floor.min(top - MIN_DB_SPAN), top.max(floor + MIN_DB_SPAN));
        a.attack = clamp_duration(a.attack, SPECTRUM_BALLISTICS);
        a.release = clamp_duration(a.release, SPECTRUM_BALLISTICS);
        a.smoothing_octaves = clamp_f32(
            a.smoothing_octaves,
            (0.0, SPECTRUM_MAX_SMOOTHING),
            defaults.smoothing_octaves,
        );
        a.peak_hold = clamp_hold(a.peak_hold);
        a.style = match a.style {
            SpectrumStyle::Line { .. } => SpectrumStyle::Line {
                points: SPECTRUM_LINE_POINTS,
            },
            SpectrumStyle::Bars { bands_per_octave } => SpectrumStyle::Bars {
                bands_per_octave: nearest(bands_per_octave as f32, &SPECTRUM_BANDS, |b| b as f32),
            },
        };
        self
    }
}

impl LoudnessMeterSettings {
    fn clamped(mut self) -> Self {
        let defaults = LoudnessMeterSettings::default();
        self.target = self
            .target
            .map(|t| clamp_f64(t, LOUDNESS_TARGET, defaults.target.unwrap_or(-14.0)));
        let a = &mut self.analysis;
        a.rms_window = clamp_duration(a.rms_window, RMS_WINDOW);
        a.peak_hold = clamp_hold(a.peak_hold);
        let (floor, top) = self.bar_range;
        let floor = clamp_f64(floor, LOUDNESS_BAR, defaults.bar_range.0);
        let top = clamp_f64(top, LOUDNESS_BAR, defaults.bar_range.1);
        let span = f64::from(MIN_DB_SPAN);
        self.bar_range = (floor.min(top - span), top.max(floor + span));
        self
    }
}

impl StereometerMeterSettings {
    fn clamped(mut self) -> Self {
        let defaults = StereometerMeterSettings::default();
        self.persistence = clamp_duration(self.persistence, STEREO_PERSISTENCE);
        self.correlation_threshold = clamp_f32(
            self.correlation_threshold,
            (-1.0, 1.0),
            defaults.correlation_threshold,
        );
        let a = &mut self.analysis;
        a.correlation_time = clamp_duration(a.correlation_time, CORRELATION_TIME);
        if let StereoScaling::Fixed { gain } = a.scaling {
            a.scaling = StereoScaling::Fixed {
                gain: clamp_f32(gain, STEREO_GAIN, 1.0),
            };
        }
        self
    }
}
