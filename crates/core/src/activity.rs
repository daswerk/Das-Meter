//! How lit a Meter is in the Smooth Look: fully while sound arrives, dimming
//! gently over about a second once it stops, brightening quickly when it
//! comes back. Settles at exactly 0 or 1, so a still picture stays still.

use std::time::Duration;

/// A block louder than this (about −80 dBFS) counts as sound.
const AUDIBLE: f32 = 1e-4;
/// Silence shorter than this (a gap between notes) changes nothing.
const HOLD: Duration = Duration::from_millis(300);
/// How long brightening and dimming take, in seconds.
const RISE: f32 = 0.15;
const FALL: f32 = 1.0;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Activity {
    /// When a block last had sound in it.
    last_sound: Option<Duration>,
    level: f32,
    /// When `level` was last brought up to date.
    at: Duration,
}

impl Activity {
    /// Takes a block the Meter was fed.
    pub fn hear(&mut self, frames: &[f32], now: Duration) {
        if frames.iter().any(|x| x.abs() > AUDIBLE) {
            self.update(now);
            self.last_sound = Some(now);
        }
    }

    /// How lit the Meter is now, 0 (dim) to 1, to 0.01.
    pub fn level(&mut self, now: Duration) -> f32 {
        self.update(now);
        (self.level * 100.0).round() / 100.0
    }

    fn update(&mut self, now: Duration) {
        let seconds = now.saturating_sub(self.at).as_secs_f32();
        self.at = now;
        let sounding = self
            .last_sound
            .is_some_and(|last| now.saturating_sub(last) <= HOLD);
        self.level = if sounding {
            (self.level + seconds / RISE).min(1.0)
        } else {
            (self.level - seconds / FALL).max(0.0)
        };
    }
}
