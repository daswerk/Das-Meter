//! Sharing Presets: one `.dasmeter-preset` file holding a Preset and the
//! custom Themes it uses, and importing such a file without overwriting
//! anything.

use serde::{Deserialize, Serialize};

use crate::curves::ReferenceCurve;
use crate::displays::DisplayRef;
use crate::presets::PresetData;
use crate::targets::Target;
use crate::theme::Theme;

/// What the file says it is.
pub const FORMAT: &str = "dasmeter-preset";
/// The layout's version.
pub const VERSION: u32 = 1;
/// The file extension.
pub const EXTENSION: &str = "dasmeter-preset";

/// A Theme embedded in the file: its own TOML, as in the themes folder.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EmbeddedTheme {
    pub toml: String,
}

/// The file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SharedPreset {
    pub format: String,
    pub version: u32,
    pub preset: PresetData,
    /// The custom Themes the Preset uses. Built-in Themes go by name only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub themes: Vec<EmbeddedTheme>,
    /// The Reference curves its Spectrums show, as in the curves folder.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub curves: Vec<EmbeddedTheme>,
    /// The Targets its Tonal Balance Meters compare with, as in the targets folder.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<EmbeddedTheme>,
}

/// What a shared Preset file holds, read.
#[derive(Clone, Debug, PartialEq)]
pub struct Imported {
    pub preset: PresetData,
    pub themes: Vec<Theme>,
    pub curves: Vec<ReferenceCurve>,
    pub targets: Vec<Target>,
}

impl SharedPreset {
    pub fn new(mut preset: PresetData, themes: Vec<Theme>) -> SharedPreset {
        // Display fingerprints without serial numbers, so a shared file doesn't
        // identify anyone's monitors.
        let strip = |d: &mut Option<DisplayRef>| *d = d.as_ref().map(DisplayRef::without_serial);
        strip(&mut preset.bar.display);
        strip(&mut preset.window.display);
        for pop_out in &mut preset.bar.pop_outs {
            strip(&mut pop_out.display);
        }
        preset.built_in = None;
        SharedPreset {
            format: FORMAT.to_owned(),
            version: VERSION,
            preset,
            themes: themes
                .iter()
                .map(|theme| EmbeddedTheme {
                    toml: theme.to_toml(),
                })
                .collect(),
            curves: Vec::new(),
            targets: Vec::new(),
        }
    }

    /// The Reference curves to embed with it.
    pub fn with_curves(mut self, curves: Vec<ReferenceCurve>) -> SharedPreset {
        self.curves = curves
            .iter()
            .map(|curve| EmbeddedTheme {
                toml: curve.to_toml(),
            })
            .collect();
        self
    }

    /// The Targets to embed with it.
    pub fn with_targets(mut self, targets: Vec<Target>) -> SharedPreset {
        self.targets = targets
            .iter()
            .map(|target| EmbeddedTheme {
                toml: target.to_toml(),
            })
            .collect();
        self
    }

    pub fn to_toml(&self) -> String {
        toml::to_string(self).expect("a shared Preset always serialises")
    }

    /// Reads a file; anything that isn't a Das-Meter Preset, or has an
    /// unreadable Theme, Reference curve or Target in it, is refused whole.
    pub fn from_toml(text: &str) -> Result<Imported, String> {
        let shared: SharedPreset = toml::from_str(text)
            .or_else(|e| toml::from_str(&crate::presets::wrap_large_ids(text)).map_err(|_| e))
            .map_err(|e: toml::de::Error| e.to_string())?;
        if shared.format != FORMAT {
            return Err("not a Das-Meter Preset".into());
        }
        if shared.preset.name.trim().is_empty() {
            return Err("the Preset has no name".into());
        }
        let themes = shared
            .themes
            .iter()
            .map(|t| Theme::from_toml(&t.toml))
            .collect::<Result<Vec<_>, _>>()?;
        let curves = shared
            .curves
            .iter()
            .map(|c| ReferenceCurve::from_toml(&c.toml))
            .collect::<Result<Vec<_>, _>>()?;
        let targets = shared
            .targets
            .iter()
            .map(|t| Target::from_toml(&t.toml))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Imported {
            preset: shared.preset.sanitised(),
            themes,
            curves,
            targets,
        })
    }
}
