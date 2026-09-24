//! Colour roles: the named colours Meters and the app are drawn with.
//!
//! Renderers never use literal colours; they ask the scene's [`Palette`] for a
//! [`Role`]. For now the only palette is the built-in Dark one; the Themes
//! ticket adds the others, the editor and per-Meter overrides.

use std::ops::Index;

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

    /// The same colour with its alpha scaled by `opacity` (0–1).
    pub fn faded(self, opacity: f32) -> Colour {
        Colour {
            a: (f32::from(self.a) * opacity.clamp(0.0, 1.0)).round() as u8,
            ..self
        }
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
}

impl Role {
    pub const ALL: [Role; 17] = [
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
            Role::SpectrumLine => Colour::rgb(0x6c, 0xc4, 0xff),
            Role::SpectrumFill => Colour::rgb(0x2c, 0x7f, 0xc0),
            Role::SpectrumPeakHold => Colour::rgb(0xe8, 0xe8, 0xe8),
            Role::LoudnessBar => Colour::rgb(0x2f, 0xbf, 0x71),
            Role::LoudnessPeak => Colour::rgb(0xe8, 0xe8, 0xe8),
            Role::LoudnessOverTarget => Colour::rgb(0xff, 0x8a, 0x3d),
            Role::CorrelationPositive => Colour::rgb(0x2f, 0xbf, 0x71),
            Role::CorrelationNegative => Colour::rgb(0xf0, 0x3e, 0x3e),
            Role::StereometerTrace => Colour::rgb(0x7f, 0xe0, 0xc8),
        };
        Palette {
            colours: Role::ALL.map(colour),
        }
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
