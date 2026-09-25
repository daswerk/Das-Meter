//! Themes: colour roles and styling, the built-ins, and their TOML files.
//!
//! Renderers never use literal colours; they ask the scene's [`Palette`] for a
//! [`Role`], and scale lines, gaps and text by the Theme's [`Styling`].

use std::ops::{Index, IndexMut};

/// An sRGB colour with straight (not premultiplied) alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Colour {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Colour {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Colour {
        Colour { r, g, b, a: 255 }
    }

    /// `#rrggbb`, or `#rrggbbaa` when not opaque.
    pub fn hex(self) -> String {
        let Colour { r, g, b, a } = self;
        if a == 255 {
            format!("#{r:02x}{g:02x}{b:02x}")
        } else {
            format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
        }
    }

    /// Reads `#rrggbb` or `#rrggbbaa`.
    pub fn from_hex(text: &str) -> Option<Colour> {
        let hex = text.strip_prefix('#')?;
        if !matches!(hex.len(), 6 | 8) || !hex.is_ascii() {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        Some(Colour {
            r: byte(0)?,
            g: byte(2)?,
            b: byte(4)?,
            a: if hex.len() == 8 { byte(6)? } else { 255 },
        })
    }

    /// The same colour with its alpha scaled by `opacity` (0–1).
    pub fn faded(self, opacity: f32) -> Colour {
        Colour {
            a: (f32::from(self.a) * opacity.clamp(0.0, 1.0)).round() as u8,
            ..self
        }
    }
}

impl serde::Serialize for Colour {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.hex())
    }
}

impl<'de> serde::Deserialize<'de> for Colour {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Colour, D::Error> {
        let text = String::deserialize(deserializer)?;
        Colour::from_hex(&text).ok_or_else(|| serde::de::Error::custom("not a #rrggbb colour"))
    }
}

impl serde::Serialize for Role {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.key())
    }
}

impl<'de> serde::Deserialize<'de> for Role {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Role, D::Error> {
        let text = String::deserialize(deserializer)?;
        Role::ALL
            .into_iter()
            .find(|role| role.key() == text)
            .ok_or_else(|| serde::de::Error::custom("not a colour role"))
    }
}

/// A named colour a Theme provides.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    // Chrome.
    Background,
    Panel,
    Grid,
    Text,
    Accent,
    // Waveform bands.
    WaveformLow,
    WaveformMid,
    WaveformHigh,
    // Spectrum.
    SpectrumLine,
    SpectrumFill,
    SpectrumPeakHold,
    // Loudness Meter.
    LoudnessBar,
    LoudnessPeak,
    LoudnessOverTarget,
    // Correlation bar.
    CorrelationPositive,
    CorrelationNegative,
    // Stereometer.
    StereometerTrace,
    // Cepstrum.
    CepstrumTrace,
}

impl Role {
    /// The role's key in a Theme file.
    pub fn key(self) -> &'static str {
        match self {
            Role::Background => "background",
            Role::Panel => "panel",
            Role::Grid => "grid",
            Role::Text => "text",
            Role::Accent => "accent",
            Role::WaveformLow => "waveform_low",
            Role::WaveformMid => "waveform_mid",
            Role::WaveformHigh => "waveform_high",
            Role::SpectrumLine => "spectrum_line",
            Role::SpectrumFill => "spectrum_fill",
            Role::SpectrumPeakHold => "spectrum_peak_hold",
            Role::LoudnessBar => "loudness_bar",
            Role::LoudnessPeak => "loudness_peak",
            Role::LoudnessOverTarget => "loudness_over_target",
            Role::CorrelationPositive => "correlation_positive",
            Role::CorrelationNegative => "correlation_negative",
            Role::StereometerTrace => "stereometer_trace",
            Role::CepstrumTrace => "cepstrum_trace",
        }
    }

    /// The role's name in the editor.
    pub fn label(self) -> &'static str {
        match self {
            Role::Background => "Background",
            Role::Panel => "Panel",
            Role::Grid => "Grid",
            Role::Text => "Text",
            Role::Accent => "Accent",
            Role::WaveformLow => "Waveform low",
            Role::WaveformMid => "Waveform mid",
            Role::WaveformHigh => "Waveform high",
            Role::SpectrumLine => "Spectrum line",
            Role::SpectrumFill => "Spectrum fill",
            Role::SpectrumPeakHold => "Spectrum peak hold",
            Role::LoudnessBar => "Loudness bar",
            Role::LoudnessPeak => "Loudness peak",
            Role::LoudnessOverTarget => "Loudness over target",
            Role::CorrelationPositive => "Correlation positive",
            Role::CorrelationNegative => "Correlation negative",
            Role::StereometerTrace => "Stereometer trace",
            Role::CepstrumTrace => "Cepstrum trace",
        }
    }

    pub const ALL: [Role; 18] = [
        Role::Background,
        Role::Panel,
        Role::Grid,
        Role::Text,
        Role::Accent,
        Role::WaveformLow,
        Role::WaveformMid,
        Role::WaveformHigh,
        Role::SpectrumLine,
        Role::SpectrumFill,
        Role::SpectrumPeakHold,
        Role::LoudnessBar,
        Role::LoudnessPeak,
        Role::LoudnessOverTarget,
        Role::CorrelationPositive,
        Role::CorrelationNegative,
        Role::StereometerTrace,
        Role::CepstrumTrace,
    ];
}

/// A colour for every [`Role`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    colours: [Colour; Role::ALL.len()],
}

impl Palette {
    /// The built-in Dark palette, the default.
    pub fn dark() -> Palette {
        let colour = |role| match role {
            Role::Background => Colour::rgb(0x11, 0x13, 0x16),
            Role::Panel => Colour::rgb(0x1a, 0x1d, 0x21),
            Role::Grid => Colour::rgb(0x33, 0x39, 0x40),
            Role::Text => Colour::rgb(0xd8, 0xdd, 0xe3),
            Role::Accent => Colour::rgb(0xff, 0xb3, 0x47),
            Role::WaveformLow => Colour::rgb(0xff, 0x4d, 0x4d),
            Role::WaveformMid => Colour::rgb(0x4d, 0xe0, 0x6b),
            Role::WaveformHigh => Colour::rgb(0x4d, 0x8d, 0xff),
            Role::SpectrumLine => Colour::rgb(0xc3, 0x9b, 0xff),
            Role::SpectrumFill => Colour::rgb(0x7a, 0x3f, 0xd6),
            Role::SpectrumPeakHold => Colour::rgb(0xe8, 0xe8, 0xe8),
            Role::LoudnessBar => Colour::rgb(0x2f, 0xbf, 0x71),
            Role::LoudnessPeak => Colour::rgb(0xe8, 0xe8, 0xe8),
            Role::LoudnessOverTarget => Colour::rgb(0xff, 0x8a, 0x3d),
            Role::CorrelationPositive => Colour::rgb(0x2f, 0xbf, 0x71),
            Role::CorrelationNegative => Colour::rgb(0xf0, 0x3e, 0x3e),
            Role::StereometerTrace => Colour::rgb(0x7f, 0xe0, 0xc8),
            Role::CepstrumTrace => Colour::rgb(0xff, 0x7a, 0xb6),
        };
        Palette {
            colours: Role::ALL.map(colour),
        }
    }
}

impl Palette {
    /// The built-in Light palette.
    pub fn light() -> Palette {
        let colour = |role| match role {
            Role::Background => Colour::rgb(0xf3, 0xf4, 0xf6),
            Role::Panel => Colour::rgb(0xff, 0xff, 0xff),
            Role::Grid => Colour::rgb(0xd5, 0xd9, 0xde),
            Role::Text => Colour::rgb(0x1d, 0x21, 0x26),
            Role::Accent => Colour::rgb(0xd9, 0x7a, 0x00),
            Role::WaveformLow => Colour::rgb(0xd6, 0x33, 0x33),
            Role::WaveformMid => Colour::rgb(0x1f, 0xa3, 0x44),
            Role::WaveformHigh => Colour::rgb(0x2d, 0x6c, 0xdf),
            Role::SpectrumLine => Colour::rgb(0x6b, 0x2f, 0xc7),
            Role::SpectrumFill => Colour::rgb(0xc5, 0xa6, 0xf2),
            Role::SpectrumPeakHold => Colour::rgb(0x4a, 0x50, 0x58),
            Role::LoudnessBar => Colour::rgb(0x1f, 0x9d, 0x5a),
            Role::LoudnessPeak => Colour::rgb(0x2a, 0x2e, 0x33),
            Role::LoudnessOverTarget => Colour::rgb(0xe0, 0x6a, 0x10),
            Role::CorrelationPositive => Colour::rgb(0x1f, 0x9d, 0x5a),
            Role::CorrelationNegative => Colour::rgb(0xd6, 0x28, 0x28),
            Role::StereometerTrace => Colour::rgb(0x0f, 0x8f, 0x7a),
            Role::CepstrumTrace => Colour::rgb(0xc2, 0x2f, 0x7a),
        };
        Palette {
            colours: Role::ALL.map(colour),
        }
    }

    /// The built-in High contrast palette: black and white chrome and colours
    /// chosen to stay apart for colour-blind viewers (blue/orange, not red/green).
    pub fn high_contrast() -> Palette {
        let colour = |role| match role {
            Role::Background => Colour::rgb(0x00, 0x00, 0x00),
            Role::Panel => Colour::rgb(0x00, 0x00, 0x00),
            Role::Grid => Colour::rgb(0x70, 0x70, 0x70),
            Role::Text => Colour::rgb(0xff, 0xff, 0xff),
            Role::Accent => Colour::rgb(0xff, 0xd6, 0x00),
            Role::WaveformLow => Colour::rgb(0xff, 0x9f, 0x1c),
            Role::WaveformMid => Colour::rgb(0xff, 0xff, 0xff),
            Role::WaveformHigh => Colour::rgb(0x3d, 0xa5, 0xff),
            Role::SpectrumLine => Colour::rgb(0xff, 0xff, 0xff),
            Role::SpectrumFill => Colour::rgb(0x3d, 0xa5, 0xff),
            Role::SpectrumPeakHold => Colour::rgb(0xff, 0xd6, 0x00),
            Role::LoudnessBar => Colour::rgb(0x3d, 0xa5, 0xff),
            Role::LoudnessPeak => Colour::rgb(0xff, 0xff, 0xff),
            Role::LoudnessOverTarget => Colour::rgb(0xff, 0x9f, 0x1c),
            Role::CorrelationPositive => Colour::rgb(0x3d, 0xa5, 0xff),
            Role::CorrelationNegative => Colour::rgb(0xff, 0x9f, 0x1c),
            Role::StereometerTrace => Colour::rgb(0xff, 0xff, 0xff),
            Role::CepstrumTrace => Colour::rgb(0x3d, 0xa5, 0xff),
        };
        Palette {
            colours: Role::ALL.map(colour),
        }
    }

    /// This palette with `overrides` taking the place of their roles.
    pub fn with(&self, overrides: &[(Role, Colour)]) -> Palette {
        let mut palette = self.clone();
        for &(role, colour) in overrides {
            palette[role] = colour;
        }
        palette
    }
}

impl IndexMut<Role> for Palette {
    fn index_mut(&mut self, role: Role) -> &mut Colour {
        &mut self.colours[role as usize]
    }
}

/// How thick lines are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LineWeight {
    Thin,
    #[default]
    Normal,
    Thick,
}

impl LineWeight {
    /// What line widths are multiplied by.
    pub fn factor(self) -> f32 {
        match self {
            LineWeight::Thin => 0.7,
            LineWeight::Normal => 1.0,
            LineWeight::Thick => 1.6,
        }
    }

    fn key(self) -> &'static str {
        match self {
            LineWeight::Thin => "thin",
            LineWeight::Normal => "normal",
            LineWeight::Thick => "thick",
        }
    }
}

/// A Theme's styling besides colours.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Styling {
    /// The window background's opacity, 0–1 (no blur). Default 1.
    pub background_opacity: f32,
    pub line: LineWeight,
    /// Space between Meters and around them, in logical px. Default 6.
    pub gap: f32,
    /// Corner radius of the Meters' panels, in logical px. Default 4.
    pub corner_radius: f32,
    /// Text size factor, 0.9–1.5. Default 1.
    pub text_scale: f32,
    /// Shape or pattern cues besides colour (hatching over target, a marker
    /// for negative correlation), for colour-blind viewers. High contrast has them.
    pub shape_cues: bool,
}

impl Default for Styling {
    fn default() -> Self {
        Styling {
            background_opacity: 1.0,
            line: LineWeight::Normal,
            gap: 6.0,
            corner_radius: 4.0,
            text_scale: 1.0,
            shape_cues: false,
        }
    }
}

/// Styling limits the editor and files are kept within.
pub const GAP_RANGE: (f32, f32) = (0.0, 24.0);
pub const CORNER_RANGE: (f32, f32) = (0.0, 16.0);
pub const TEXT_SCALE_RANGE: (f32, f32) = (0.9, 1.5);

impl Styling {
    /// These values kept within their ranges.
    pub fn clamped(self) -> Styling {
        let clamp = |v: f32, (lo, hi): (f32, f32), default: f32| {
            if v.is_nan() { default } else { v.clamp(lo, hi) }
        };
        let d = Styling::default();
        Styling {
            background_opacity: clamp(self.background_opacity, (0.0, 1.0), 1.0),
            gap: clamp(self.gap, GAP_RANGE, d.gap),
            corner_radius: clamp(self.corner_radius, CORNER_RANGE, d.corner_radius),
            text_scale: clamp(self.text_scale, TEXT_SCALE_RANGE, 1.0),
            ..self
        }
    }
}

/// A named set of colours and styling.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub name: String,
    pub palette: Palette,
    pub styling: Styling,
}

/// The built-in Themes' names.
pub const DARK: &str = "Dark";
pub const LIGHT: &str = "Light";
pub const HIGH_CONTRAST: &str = "High contrast";

/// The Theme file layout's version.
const FILE_VERSION: i64 = 1;

impl Theme {
    pub fn dark() -> Theme {
        Theme {
            name: DARK.to_owned(),
            palette: Palette::dark(),
            styling: Styling::default(),
        }
    }

    pub fn light() -> Theme {
        Theme {
            name: LIGHT.to_owned(),
            palette: Palette::light(),
            styling: Styling::default(),
        }
    }

    pub fn high_contrast() -> Theme {
        Theme {
            name: HIGH_CONTRAST.to_owned(),
            palette: Palette::high_contrast(),
            styling: Styling {
                line: LineWeight::Thick,
                shape_cues: true,
                ..Styling::default()
            },
        }
    }

    /// The built-ins, in the order they're listed.
    pub fn built_ins() -> [Theme; 3] {
        [Theme::dark(), Theme::light(), Theme::high_contrast()]
    }

    /// The Theme as a TOML file.
    pub fn to_toml(&self) -> String {
        let mut file = toml::Table::new();
        file.insert("version".into(), FILE_VERSION.into());
        file.insert("name".into(), self.name.clone().into());
        let mut colours = toml::Table::new();
        for role in Role::ALL {
            colours.insert(role.key().into(), self.palette[role].hex().into());
        }
        file.insert("colours".into(), colours.into());
        let s = &self.styling;
        let mut styling = toml::Table::new();
        styling.insert(
            "background_opacity".into(),
            f64::from(s.background_opacity).into(),
        );
        styling.insert("line".into(), s.line.key().into());
        styling.insert("gap".into(), f64::from(s.gap).into());
        styling.insert("corner_radius".into(), f64::from(s.corner_radius).into());
        styling.insert("text_scale".into(), f64::from(s.text_scale).into());
        styling.insert("shape_cues".into(), s.shape_cues.into());
        file.insert("styling".into(), styling.into());
        file.to_string()
    }

    /// Reads a Theme file. Anything it leaves out comes from Dark; values out
    /// of range are kept within it.
    pub fn from_toml(text: &str) -> Result<Theme, String> {
        let file: toml::Table = text.parse().map_err(|e: toml::de::Error| e.to_string())?;
        let version = file.get("version").and_then(toml::Value::as_integer);
        if version.is_some_and(|v| v > FILE_VERSION) {
            return Err(format!(
                "made by a newer Das-Meter (version {})",
                version.unwrap_or(0)
            ));
        }
        let name = file
            .get("name")
            .and_then(toml::Value::as_str)
            .filter(|n| !n.trim().is_empty())
            .ok_or("no name")?
            .to_owned();
        let mut theme = Theme {
            name,
            ..Theme::dark()
        };
        if let Some(colours) = file.get("colours").and_then(toml::Value::as_table) {
            for role in Role::ALL {
                if let Some(colour) = colours
                    .get(role.key())
                    .and_then(toml::Value::as_str)
                    .and_then(Colour::from_hex)
                {
                    theme.palette[role] = colour;
                }
            }
        }
        if let Some(styling) = file.get("styling").and_then(toml::Value::as_table) {
            let number = |key: &str| {
                styling.get(key).and_then(|v| {
                    v.as_float()
                        .or_else(|| v.as_integer().map(|i| i as f64))
                        .map(|f| f as f32)
                })
            };
            let s = &mut theme.styling;
            s.background_opacity = number("background_opacity").unwrap_or(s.background_opacity);
            s.gap = number("gap").unwrap_or(s.gap);
            s.corner_radius = number("corner_radius").unwrap_or(s.corner_radius);
            s.text_scale = number("text_scale").unwrap_or(s.text_scale);
            s.line = match styling.get("line").and_then(toml::Value::as_str) {
                Some("thin") => LineWeight::Thin,
                Some("thick") => LineWeight::Thick,
                _ => s.line,
            };
            s.shape_cues = styling
                .get("shape_cues")
                .and_then(toml::Value::as_bool)
                .unwrap_or(s.shape_cues);
            *s = s.clamped();
        }
        Ok(theme)
    }
}

impl Default for Palette {
    fn default() -> Self {
        Palette::dark()
    }
}

impl Index<Role> for Palette {
    type Output = Colour;

    fn index(&self, role: Role) -> &Colour {
        &self.colours[role as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_index_their_own_slot() {
        for (i, role) in Role::ALL.into_iter().enumerate() {
            assert_eq!(role as usize, i);
        }
    }
}
