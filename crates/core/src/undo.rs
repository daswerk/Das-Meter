//! Undo and Redo for what a Preset saves and the Themes it uses: each change
//! is a step back to the state before it. Changes of one kind that follow
//! close on each other (a slider dragged, a colour picked) are one step.
//! Where the window is isn't part of it: moving it is never a step.

use std::time::Duration;

use crate::presets::PresetData;
use crate::theme::Theme;

/// Changes of one kind within this of the last are the same step.
const SAME_STEP: Duration = Duration::from_millis(800);
/// The most steps kept.
const STEPS: usize = 100;

/// What a step brings back: the Preset's state (its window left where it
/// is) and the folder Themes it uses, as they were.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub data: PresetData,
    pub themes: Vec<Theme>,
}

/// A change's kind: which event it was, its fields aside.
pub fn kind<T>(event: &T) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::mem::discriminant(event).hash(&mut hasher);
    hasher.finish()
}

#[derive(Default)]
pub struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// The state now, as the last step left it.
    present: Option<Snapshot>,
    /// The last change's kind (see [`kind`]), and when.
    last: Option<(u64, Duration)>,
}

impl History {
    /// Starts over from `present` (a Preset opened): nothing to undo.
    pub fn reset(&mut self, present: Snapshot) {
        *self = History {
            present: Some(present),
            ..History::default()
        };
    }

    /// The state is `current` now without a change to undo (the folder's
    /// Themes read again).
    pub fn settle(&mut self, current: Snapshot) {
        self.present = Some(current);
    }

    /// A change of `kind` at `now` left the state at `current`.
    pub fn changed(&mut self, kind: u64, current: Snapshot, now: Duration) {
        let Some(before) = self.present.take() else {
            self.present = Some(current);
            return;
        };
        if before == current {
            self.present = Some(before);
            return;
        }
        let same_step = self
            .last
            .is_some_and(|(k, at)| k == kind && now.saturating_sub(at) < SAME_STEP);
        if !same_step || self.undo.is_empty() {
            self.undo.push(before);
            if self.undo.len() > STEPS {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.present = Some(current);
        self.last = Some((kind, now));
    }

    /// The state to go back to from `current`, if there's one.
    pub fn undo(&mut self, current: Snapshot) -> Option<Snapshot> {
        step(
            &mut self.undo,
            &mut self.redo,
            current,
            &mut self.present,
            &mut self.last,
        )
    }

    /// The state to go forward to again from `current`, if there's one.
    pub fn redo(&mut self, current: Snapshot) -> Option<Snapshot> {
        step(
            &mut self.redo,
            &mut self.undo,
            current,
            &mut self.present,
            &mut self.last,
        )
    }
}

/// Takes the newest state from `from` that differs from `current`, keeping
/// `current` on `to` to come back to.
fn step(
    from: &mut Vec<Snapshot>,
    to: &mut Vec<Snapshot>,
    current: Snapshot,
    present: &mut Option<Snapshot>,
    last: &mut Option<(u64, Duration)>,
) -> Option<Snapshot> {
    while let Some(state) = from.pop() {
        if state != current {
            to.push(current);
            *present = Some(state.clone());
            // The next change starts a step of its own.
            *last = None;
            return Some(state);
        }
    }
    None
}
