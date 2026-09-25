//! The Waveform's analyser: a scrolling envelope with three band energies per column.
//!
//! Each column covers a fixed slice of time ([`COLUMNS_PER_SECOND`]), so the
//! time span sets the column count. Per column and trace it keeps the sample
//! minimum and maximum (the envelope) and the RMS of a low, mid and high band,
//! split by Linkwitz-Riley crossovers, for RGB and Rekordbox colouring.

use std::collections::VecDeque;
use std::time::Duration;

use crate::ChannelView;

/// Columns per second of audio: each column is 5 ms.
pub const COLUMNS_PER_SECOND: u32 = 200;

/// Allowed time spans.
pub const MIN_SPAN: Duration = Duration::from_secs(1);
pub const MAX_SPAN: Duration = Duration::from_secs(30);

/// How amplitude maps to height.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum WaveformScale {
    #[default]
    Linear,
    /// Decibels from −60 dBFS (bottom) to 0 dBFS (top).
    Decibels,
}

/// The Waveform's settings that affect analysis.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct WaveformSettings {
    /// 1 to 30 s. Default 4 s.
    #[serde(with = "crate::seconds")]
    pub span: Duration,
    pub channel_view: ChannelView,
    pub scale: WaveformScale,
    /// Linear display gain (zoom). Default 1.
    pub gain: f32,
    /// Crossover between the low and mid bands, in Hz. Default 200.
    pub low_crossover: f32,
    /// Crossover between the mid and high bands, in Hz. Default 2000.
    pub high_crossover: f32,
}

impl Default for WaveformSettings {
    fn default() -> Self {
        WaveformSettings {
            span: Duration::from_secs(4),
            channel_view: ChannelView::Mono,
            scale: WaveformScale::Linear,
            gain: 1.0,
            low_crossover: 200.0,
            high_crossover: 2_000.0,
        }
    }
}

/// Floor of the dB scale.
const DB_SCALE_FLOOR: f32 = -60.0;

impl WaveformSettings {
    /// Number of columns the time span holds.
    pub fn columns(&self) -> usize {
        let span = self.span.clamp(MIN_SPAN, MAX_SPAN);
        (span.as_secs_f64() * f64::from(COLUMNS_PER_SECOND)).round() as usize
    }

    /// Maps a sample value to a display height in −1..1, applying gain and scale.
    pub fn height(&self, sample: f32) -> f32 {
        let x = (sample * self.gain).clamp(-1.0, 1.0);
        match self.scale {
            WaveformScale::Linear => x,
            WaveformScale::Decibels => {
                let magnitude = x.abs();
                if magnitude <= 0.0 {
                    return 0.0;
                }
                let db = 20.0 * magnitude.log10();
                let h = ((db - DB_SCALE_FLOOR) / -DB_SCALE_FLOOR).clamp(0.0, 1.0);
                h.copysign(x)
            }
        }
    }
}

/// One trace's slice of time.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct WaveformColumn {
    pub min: f32,
    pub max: f32,
    /// RMS of each band: below the low crossover, between the two, above the high one.
    pub low: f32,
    pub mid: f32,
    pub high: f32,
}

/// A biquad in transposed direct form II.
#[derive(Clone, Copy, Default)]
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    z: [f32; 2],
}

impl Biquad {
    /// Butterworth (Q = 1/√2) low- or high-pass, from the RBJ cookbook.
    fn butterworth(sample_rate: u32, frequency: f32, high_pass: bool) -> Biquad {
        let nyquist = sample_rate as f32 / 2.0;
        let w0 = std::f32::consts::TAU * frequency.clamp(1.0, nyquist * 0.99) / sample_rate as f32;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / std::f32::consts::SQRT_2;
        let a0 = 1.0 + alpha;
        let b = if high_pass {
            [(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0]
        } else {
            [(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0]
        };
        Biquad {
            b: [b[0] / a0, b[1] / a0, b[2] / a0],
            a: [-2.0 * cos / a0, (1.0 - alpha) / a0],
            z: [0.0; 2],
        }
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// A Linkwitz-Riley 4th-order filter: two identical Butterworth biquads in series.
#[derive(Clone, Copy, Default)]
struct Lr4([Biquad; 2]);

impl Lr4 {
    fn new(sample_rate: u32, frequency: f32, high_pass: bool) -> Lr4 {
        let stage = Biquad::butterworth(sample_rate, frequency, high_pass);
        Lr4([stage, stage])
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.0[0].run(x);
        self.0[1].run(y)
    }
}

/// Splits one trace into three bands and accumulates the current column.
#[derive(Clone, Copy)]
struct TraceState {
    low: Lr4,
    mid_high_pass: Lr4,
    mid_low_pass: Lr4,
    high: Lr4,
    min: f32,
    max: f32,
    squares: [f64; 3],
}

impl TraceState {
    fn new(sample_rate: u32, settings: &WaveformSettings) -> TraceState {
        TraceState {
            low: Lr4::new(sample_rate, settings.low_crossover, false),
            mid_high_pass: Lr4::new(sample_rate, settings.low_crossover, true),
            mid_low_pass: Lr4::new(sample_rate, settings.high_crossover, false),
            high: Lr4::new(sample_rate, settings.high_crossover, true),
            min: f32::MAX,
            max: f32::MIN,
            squares: [0.0; 3],
        }
    }

    fn push(&mut self, x: f32) {
        self.min = self.min.min(x);
        self.max = self.max.max(x);
        let low = self.low.run(x);
        let mid = self.mid_low_pass.run(self.mid_high_pass.run(x));
        let high = self.high.run(x);
        for (sum, band) in self.squares.iter_mut().zip([low, mid, high]) {
            *sum += f64::from(band) * f64::from(band);
        }
    }

    fn finish(&mut self, frames: usize) -> WaveformColumn {
        let rms = |sum: f64| (sum / frames as f64).sqrt() as f32;
        let column = WaveformColumn {
            min: self.min,
            max: self.max,
            low: rms(self.squares[0]),
            mid: rms(self.squares[1]),
            high: rms(self.squares[2]),
        };
        self.min = f32::MAX;
        self.max = f32::MIN;
        self.squares = [0.0; 3];
        column
    }
}

/// Stereo frames in, scrolling columns out.
pub struct WaveformAnalyser {
    sample_rate: u32,
    settings: WaveformSettings,
    /// Frames per column, as a fraction so long runs don't drift.
    frames_per_column: f64,
    column_frames: usize,
    /// Frame count at which the current column ends.
    column_end: f64,
    frame: u64,
    traces: [TraceState; 2],
    /// Completed columns per trace, oldest first.
    columns: [VecDeque<WaveformColumn>; 2],
    /// Columns completed since the start: the newest column's number plus one.
    completed: u64,
}

impl WaveformAnalyser {
    pub fn new(sample_rate: u32, settings: WaveformSettings) -> WaveformAnalyser {
        let frames_per_column = f64::from(sample_rate) / f64::from(COLUMNS_PER_SECOND);
        let capacity = settings.columns();
        WaveformAnalyser {
            sample_rate,
            settings,
            frames_per_column,
            column_frames: 0,
            column_end: frames_per_column,
            frame: 0,
            traces: [TraceState::new(sample_rate, &settings); 2],
            columns: [
                VecDeque::with_capacity(capacity),
                VecDeque::with_capacity(capacity),
            ],
            completed: 0,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn settings(&self) -> &WaveformSettings {
        &self.settings
    }

    /// Restarts for a new sample rate.
    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        *self = WaveformAnalyser::new(sample_rate, self.settings);
    }

    /// Applies new settings. Only a new Channel View or crossover restarts the
    /// history; a new span keeps the newest columns that still fit.
    pub fn set_settings(&mut self, settings: WaveformSettings) {
        let old = self.settings;
        if settings.channel_view != old.channel_view
            || settings.low_crossover != old.low_crossover
            || settings.high_crossover != old.high_crossover
        {
            *self = WaveformAnalyser::new(self.sample_rate, settings);
            return;
        }
        self.settings = settings;
        let capacity = settings.columns();
        for columns in &mut self.columns {
            while columns.len() > capacity {
                columns.pop_front();
            }
        }
    }

    /// Feeds interleaved stereo frames. A trailing half frame is ignored.
    pub fn process(&mut self, interleaved: &[f32]) {
        let view = self.settings.channel_view;
        let traces = view.traces();
        let capacity = self.settings.columns();
        for frame in interleaved.chunks_exact(2) {
            let split = view.split(frame[0], frame[1]);
            for (state, &x) in self.traces.iter_mut().zip(&split).take(traces) {
                state.push(x);
            }
            self.column_frames += 1;
            self.frame += 1;
            if self.frame as f64 >= self.column_end {
                for (state, columns) in self.traces.iter_mut().zip(&mut self.columns).take(traces) {
                    if columns.len() == capacity {
                        columns.pop_front();
                    }
                    columns.push_back(state.finish(self.column_frames));
                }
                self.column_frames = 0;
                self.column_end += self.frames_per_column;
                self.completed += 1;
            }
        }
    }

    /// Number of traces the Channel View draws.
    pub fn traces(&self) -> usize {
        self.settings.channel_view.traces()
    }

    /// Columns completed since the start (or the last restart). With the
    /// columns' count it numbers each column in time, so a renderer can merge
    /// them into pixels by time rather than by age, and merged pixels keep
    /// their content as they scroll.
    pub fn completed(&self) -> u64 {
        self.completed
    }

    /// Completed columns of one trace, oldest first. At most [`WaveformSettings::columns`].
    pub fn columns(&self, trace: usize) -> impl ExactSizeIterator<Item = &WaveformColumn> {
        self.columns[trace].iter()
    }
}
