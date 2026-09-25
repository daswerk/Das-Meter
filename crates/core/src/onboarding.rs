//! First launch (spec: First launch): the welcome card, Start listening, the
//! silence hint, the one-time "Send Plugin is sending" note and the virtual
//! desktop tip. What's shown once is remembered in `settings.toml`
//! ([`FirstLaunch`]); the rest lasts for this run.

use std::time::Duration;

use crate::presets::FirstLaunch;
use crate::sources::{SendPlugin, SendPluginState};

/// How long System Capture must hear nothing before the silence hint.
pub const SILENCE_HINT_AFTER: Duration = Duration::from_secs(10);

/// A card next to the Bar (or on the Window). One shows at a time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Card {
    /// The welcome card: what Das-Meter does, Start listening, Install Send
    /// Plugin…, Learn more. Until Start listening, nothing is captured.
    Welcome,
    /// System Capture has heard nothing for a while: play something, or
    /// switch to Send Plugins (ASIO), or check the privacy setting (macOS).
    SilenceHint,
    /// A Send Plugin started sending while on System Capture: switch?
    SendPluginFound { name: String },
}

/// The first-launch state for this run.
#[derive(Clone, Debug, Default)]
pub struct Onboarding {
    welcome: bool,
    /// When System Capture went live without having heard anything since.
    silent_since: Option<Duration>,
    /// Sound arrived since capture started: no silence hint this run.
    heard: bool,
    hint: bool,
    hint_closed: bool,
    found: Option<String>,
    /// A Windows tray notification to show (the virtual desktop tip).
    desktop_tip: bool,
}

/// Whether `settings.toml` needs writing after a change.
#[must_use]
pub struct Save(pub bool);

impl Onboarding {
    /// After `settings.toml` was read: the welcome card shows until it's
    /// answered once.
    pub fn loaded(&mut self, flags: &FirstLaunch) {
        self.welcome = !flags.welcome_shown;
    }

    /// Whether System Capture may run: not before Start listening, so the
    /// macOS permission prompt only follows the explanation.
    pub fn may_capture(flags: &FirstLaunch) -> bool {
        flags.listening_started
    }

    pub fn start_listening(&mut self, flags: &mut FirstLaunch) -> Save {
        self.welcome = false;
        let changed = !flags.listening_started || !flags.welcome_shown;
        flags.listening_started = true;
        flags.welcome_shown = true;
        Save(changed)
    }

    /// ✕ on the welcome card: nothing is captured, and the Meters offer
    /// Start listening instead.
    pub fn close_welcome(&mut self, flags: &mut FirstLaunch) -> Save {
        self.welcome = false;
        let changed = !flags.welcome_shown;
        flags.welcome_shown = true;
        Save(changed)
    }

    /// Help ▸ Show welcome.
    pub fn show_welcome(&mut self) {
        self.welcome = true;
    }

    /// System Capture (re)started delivering audio.
    pub fn capture_started(&mut self, now: Duration) {
        if !self.heard {
            self.silent_since.get_or_insert(now);
        }
    }

    /// Capture stopped, or Listen to left System Capture.
    pub fn capture_stopped(&mut self) {
        self.silent_since = None;
        self.hint = false;
    }

    /// A System Capture block. Returns whether the card changed.
    pub fn audio(&mut self, audible: bool, now: Duration) -> bool {
        if audible {
            self.heard = true;
            self.silent_since = None;
            return std::mem::take(&mut self.hint);
        }
        let due = self
            .silent_since
            .is_some_and(|since| now.saturating_sub(since) >= SILENCE_HINT_AFTER);
        if due && !self.hint && !self.hint_closed && !self.heard {
            self.hint = true;
            return true;
        }
        false
    }

    /// ✕ on the silence hint: not again this run.
    pub fn close_hint(&mut self) {
        self.hint = false;
        self.hint_closed = true;
    }

    /// The Send Plugins listed while on System Capture: the first one that
    /// sends brings the note, once per install. Returns whether to save.
    pub fn send_plugins(&mut self, listed: &[SendPlugin], flags: &mut FirstLaunch) -> Save {
        if flags.send_plugin_note_shown {
            return Save(false);
        }
        let Some(sending) = listed.iter().find(|p| p.state == SendPluginState::Live) else {
            return Save(false);
        };
        self.found = Some(sending.name.clone());
        flags.send_plugin_note_shown = true;
        Save(true)
    }

    /// Switch or ✕ on the Send Plugin note.
    pub fn close_found(&mut self) {
        self.found = None;
    }

    /// Whether the Send Plugins still need listing on System Capture, for the note.
    pub fn wants_send_plugins(flags: &FirstLaunch) -> bool {
        !flags.send_plugin_note_shown
    }

    /// Windows: the user switched to a virtual desktop without the Bar. The
    /// tip shows once per install. Returns whether to save.
    pub fn desktop_without_bar(&mut self, flags: &mut FirstLaunch) -> Save {
        if flags.virtual_desktop_tip_shown {
            return Save(false);
        }
        flags.virtual_desktop_tip_shown = true;
        self.desktop_tip = true;
        Save(true)
    }

    /// The tray notification to show now, once.
    pub fn take_desktop_tip(&mut self) -> bool {
        std::mem::take(&mut self.desktop_tip)
    }

    /// The card to show: the welcome card first, then a found Send Plugin,
    /// then the silence hint.
    pub fn card(&self) -> Option<Card> {
        if self.welcome {
            Some(Card::Welcome)
        } else if let Some(name) = &self.found {
            Some(Card::SendPluginFound { name: name.clone() })
        } else if self.hint {
            Some(Card::SilenceHint)
        } else {
            None
        }
    }
}
