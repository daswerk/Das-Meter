//! Listen to and Sources: which audio each Meter shows.
//!
//! Listen to is app-wide: System Capture or Send Plugins, never mixed. On Send
//! Plugins, each Meter picks its own Send Plugin; without a pick it follows the
//! other Meters, takes the only one there is, or asks.

/// Where the app's audio comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ListenTo {
    /// Everything the computer plays through its default output.
    #[default]
    SystemCapture,
    /// DAW tracks, through the Das-Meter Send plug-in. Each Meter picks one.
    SendPlugins,
}

/// A Send Plugin as the transport lists it, fed in with [`crate::Event::SendPlugins`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SendPlugin {
    /// Stays the same across project reloads; a duplicated track gets a new one.
    pub id: u64,
    pub name: String,
    /// `0x00RRGGBB`.
    pub colour: u32,
    pub mono: bool,
    pub sample_rate: u32,
    pub state: SendPluginState,
    /// It writes an older layout than the app reads: listed, but it can't be listened to.
    pub outdated: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendPluginState {
    /// Audio is flowing.
    Live,
    /// There, but not processing (transport stopped, bypassed).
    Idle,
    /// Its DAW closed or crashed, or its heartbeat stopped.
    Gone,
}

impl SendPlugin {
    /// Listed and able to send: not gone and not outdated.
    pub(crate) fn usable(&self) -> bool {
        self.state != SendPluginState::Gone && !self.outdated
    }

    /// How it is listed in a Meter's Source list.
    pub fn label(&self) -> String {
        if self.outdated {
            format!("{} (outdated — restart your DAW)", self.name)
        } else {
            self.name.clone()
        }
    }
}

/// A Meter's pick: the Send Plugin's ID, and its name for finding it again
/// (a Preset loaded later) and for "Waiting for <name>".
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Pick {
    pub id: u64,
    pub name: String,
}

impl Pick {
    pub(crate) fn of(plugin: &SendPlugin) -> Pick {
        Pick {
            id: plugin.id,
            name: plugin.name.clone(),
        }
    }
}

/// What a Meter shows on Send Plugins.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Resolved<'a> {
    /// This Send Plugin's audio (silence while it is idle).
    Plugin(&'a SendPlugin),
    /// The picked Send Plugin isn't there: "Waiting for <name>".
    Waiting(String),
    /// Several Send Plugins and no pick: "Pick a Send Plugin".
    Choose,
    /// None at all: "No Send Plugins yet".
    Nothing,
}

/// Finds a pick among the listed Send Plugins: a usable one by ID, else a
/// usable one by name (a Preset whose Send Plugin got a new ID, or a DAW that
/// crashed and came back while its old slot is still listed as gone), else
/// the gone one by ID. `None` if none of these.
pub(crate) fn find<'a>(pick: &Pick, listed: &'a [SendPlugin]) -> Option<&'a SendPlugin> {
    let usable = || listed.iter().filter(|plugin| plugin.usable());
    usable()
        .find(|plugin| plugin.id == pick.id)
        .or_else(|| usable().find(|plugin| plugin.name == pick.name))
        .or_else(|| listed.iter().find(|plugin| plugin.id == pick.id))
}

/// What a Meter shows: its own pick, else the pick it follows (the first
/// other Meter's), else the only usable Send Plugin, else a choice or nothing.
pub(crate) fn resolve<'a>(
    pick: Option<&Pick>,
    followed: Option<&Pick>,
    listed: &'a [SendPlugin],
) -> Resolved<'a> {
    if let Some(pick) = pick.or(followed) {
        return match find(pick, listed) {
            Some(plugin) if plugin.usable() => Resolved::Plugin(plugin),
            Some(plugin) => Resolved::Waiting(plugin.name.clone()),
            None => Resolved::Waiting(pick.name.clone()),
        };
    }
    let mut usable = listed.iter().filter(|plugin| plugin.usable());
    match (usable.next(), usable.next()) {
        (Some(only), None) => Resolved::Plugin(only),
        (Some(_), Some(_)) => Resolved::Choose,
        (None, _) if listed.iter().any(|p| p.state != SendPluginState::Gone) => Resolved::Choose,
        (None, _) => Resolved::Nothing,
    }
}
