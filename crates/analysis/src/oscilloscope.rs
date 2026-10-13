//! The Oscilloscope's analyser: the waveform itself over a short window,
//! standing still on a rising edge.
//!
//! Each [`update`](OscilloscopeAnalyser::update) looks back through the
//! audio for the latest rising zero crossing of the first trace that still
//! has a whole window after it, found to a fraction of a sample, and shows
//! the window from there. A periodic wave then looks the same at every
//! update, whenever its blocks arrive. With no crossing (or the trigger off)
//! it shows the newest window. Auto gain lifts quiet audio: it falls at
//! once when the audio gets louder and rises slowly when it gets quieter.

use std::collections::VecDeque;
use std::time::Duration;

use crate::ChannelView;

/// Points per trace in the draw buffer.
pub const POINTS: usize = 512;
/// The shortest and longest windows.
pub const WINDOW_LIMITS: (Duration, Duration) =
    (Duration::from_millis(1), Duration::from_millis(200));
/// The most auto gain lifts quiet audio, in dB.
pub const MAX_GAIN_DB: f32 = 48.0;
/// Auto gain brings the peak up to this.
const TARGET_PEAK: f32 = 0.9;
/// Time constant of auto gain rising when the audio gets quieter.
const GAIN_RELEASE: Duration = Duration::from_millis(500);
/// Quieter than this (about −80 dBFS) is silence: auto gain holds.
const SILENCE: f32 = 1e-4;
/// A crossing counts once the wave has been this far (of its peak) below 0,
/// so noise near 0 doesn't trigger.
const HYSTERESIS: f32 = 0.05;
/// The lowest frequency whose period is searched for a crossing, in Hz.
const LOWEST_TRIGGER: u32 = 20;
/// Drawn values are rounded to this: finer would only redraw for nothing.
const STEP: f32 = 0.001;

/// The Oscilloscope's settings that affect analysis.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct OscilloscopeSettings {
    /// Which traces: mono (default), L/R or M/S.
    pub channels: ChannelView,
    /// The time across the Meter, 1 to 200 ms. Default 20 ms.
    #[serde(with = "crate::seconds")]
    pub window: Duration,
    /// Whether the picture stands still on a rising edge. Default on.
    pub trigger: bool,
    /// Whether quiet audio is lifted to fill the height. Default on.
    pub auto_gain: bool,
}

impl Default for OscilloscopeSettings {
    fn default() -> Self {
        OscilloscopeSettings {
            channels: ChannelView::Mono,
            window: Duration::from_millis(20),
            trigger: true,
            auto_gain: true,
        }
    }
}

/// What the renderer draws.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Oscilloscope {
    /// One trace per channel, [`POINTS`] values each from the left edge to
    /// the right, with the gain applied: −1 to 1 is the full height.
    pub traces: Vec<Vec<f32>>,
    /// The time across.
    pub window: Duration,
    /// Whether it found a rising edge to stand still on.
    pub triggered: bool,
    /// The gain applied, in dB, to 0.1 dB: 0 with auto gain off.
    pub gain_db: f32,
}

/// Stereo frames in, [`Oscilloscope`] out.
pub struct OscilloscopeAnalyser {
    sample_rate: u32,
    settings: OscilloscopeSettings,
    /// The newest audio of each trace, oldest first.
    history: [VecDeque<f32>; 2],
    /// Linear auto gain.
    gain: f32,
    /// Frames since the last update, for the gain's time constant.
    since_update: usize,
    view: Oscilloscope,
}

impl OscilloscopeAnalyser {
    pub fn new(sample_rate: u32, settings: OscilloscopeSettings) -> OscilloscopeAnalyser {
        OscilloscopeAnalyser {
            sample_rate,
            settings: settings.clamped(),
            history: [VecDeque::new(), VecDeque::new()],
            gain: 1.0,
            since_update: 0,
            view: Oscilloscope::default(),
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn settings(&self) -> &OscilloscopeSettings {
        &self.settings
    }

    pub fn set_settings(&mut self, settings: OscilloscopeSettings) {
        let settings = settings.clamped();
        if settings.channels != self.settings.channels {
            self.history.iter_mut().for_each(VecDeque::clear);
        }
        if !settings.auto_gain {
            self.gain = 1.0;
        }
        self.settings = settings;
    }

    /// Takes interleaved stereo frames.
    pub fn process(&mut self, frames: &[f32]) {
        let capacity = self.capacity();
        let traces = self.settings.channels.traces();
        for frame in frames.chunks_exact(2) {
            let split = self.settings.channels.split(frame[0], frame[1]);
            for (history, value) in self.history.iter_mut().zip(split).take(traces) {
                history.push_back(value);
            }
        }
        for history in &mut self.history {
            while history.len() > capacity {
                history.pop_front();
            }
        }
        self.since_update += frames.len() / 2;
    }

    /// The picture now.
    pub fn update(&mut self) -> &Oscilloscope {
        let span = self.window_samples();
        let traces = self.settings.channels.traces();
        let len = self.history[0].len();
        let (start, triggered) = if len > span {
            match self.settings.trigger.then(|| self.trigger(span)).flatten() {
                Some(start) => (start, true),
                None => (((len - 1) - span) as f64, false),
            }
        } else {
            (0.0, false)
        };
        let mut values: Vec<Vec<f32>> = (0..traces)
            .map(|t| resample(&self.history[t], start, span))
            .collect();
        let peak = values.iter().flatten().fold(0.0f32, |m, x| m.max(x.abs()));
        self.follow_gain(peak);
        let gain = self.gain;
        for value in values.iter_mut().flatten() {
            *value = round_to((*value * gain).clamp(-1.0, 1.0), STEP);
        }
        self.since_update = 0;
        self.view = Oscilloscope {
            traces: values,
            window: self.settings.window,
            triggered,
            gain_db: if self.settings.auto_gain {
                round_to(20.0 * gain.log10(), 0.1)
            } else {
                0.0
            },
        };
        &self.view
    }

    /// Samples across the window.
    fn window_samples(&self) -> usize {
        ((self.settings.window.as_secs_f64() * f64::from(self.sample_rate)).round() as usize).max(2)
    }

    /// Samples kept: the window, and room before it to find a crossing of
    /// the lowest frequency (or another window, if that's longer).
    fn capacity(&self) -> usize {
        let span = self.window_samples();
        span + span.max((self.sample_rate / LOWEST_TRIGGER) as usize) + 2
    }

    /// Where the window starts, to a fraction of a sample: the latest rising
    /// zero crossing of the first trace with a whole window after it.
    fn trigger(&self, span: usize) -> Option<f64> {
        let history = &self.history[0];
        let last_start = history.len() - 1 - span;
        let peak = history.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        if peak < SILENCE {
            return None;
        }
        let threshold = HYSTERESIS * peak;
        let mut armed = false;
        let mut found = None;
        for i in 1..=last_start + 1 {
            let (before, after) = (history[i - 1], history[i]);
            if before < -threshold {
                armed = true;
            }
            if armed && before < 0.0 && after >= 0.0 {
                let fraction = f64::from(-before / (after - before));
                let start = (i - 1) as f64 + fraction;
                if start <= last_start as f64 {
                    found = Some(start);
                }
                armed = false;
            }
        }
        found
    }

    /// Auto gain: down at once to keep `peak` in, up slowly.
    fn follow_gain(&mut self, peak: f32) {
        if !self.settings.auto_gain {
            self.gain = 1.0;
            return;
        }
        if peak < SILENCE {
            return;
        }
        let max = 10f32.powf(MAX_GAIN_DB / 20.0);
        let target = (TARGET_PEAK / peak).clamp(1.0, max);
        if target < self.gain {
            self.gain = target;
        } else {
            let seconds = self.since_update as f32 / self.sample_rate as f32;
            let rise = 1.0 - (-seconds / GAIN_RELEASE.as_secs_f32()).exp();
            self.gain += (target - self.gain) * rise;
        }
    }
}

impl OscilloscopeSettings {
    /// The settings with the window within [`WINDOW_LIMITS`].
    pub fn clamped(self) -> OscilloscopeSettings {
        OscilloscopeSettings {
            window: self.window.clamp(WINDOW_LIMITS.0, WINDOW_LIMITS.1),
            ..self
        }
    }
}

/// [`POINTS`] values across `span` samples of `history` from `start`. Where
/// the points are further apart than two samples, each takes the sample
/// furthest from 0 around it, so peaks don't drop out.
fn resample(history: &VecDeque<f32>, start: f64, span: usize) -> Vec<f32> {
    if history.is_empty() {
        return vec![0.0; POINTS];
    }
    let last = history.len() - 1;
    let step = span as f64 / (POINTS - 1) as f64;
    let at = |i: usize| history[i.min(last)];
    (0..POINTS)
        .map(|k| {
            let position = start + k as f64 * step;
            if step > 2.0 {
                let from = (position - step / 2.0).max(0.0).round() as usize;
                let to = ((position + step / 2.0).round() as usize).min(last);
                (from..=to.max(from))
                    .map(at)
                    .fold(0.0f32, |m, x| if x.abs() > m.abs() { x } else { m })
            } else {
                let i = position.floor() as usize;
                let fraction = (position - i as f64) as f32;
                at(i) + (at(i + 1) - at(i)) * fraction
            }
        })
        .collect()
}

fn round_to(value: f32, step: f32) -> f32 {
    (value / step).round() * step
}
