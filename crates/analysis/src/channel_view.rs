//! Which channels a Meter shows: the Channel View of Waveform and Spectrum.

/// How a stereo Source is split into the traces a Meter draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ChannelView {
    /// One trace: `(L + R) / 2`.
    #[default]
    Mono,
    /// Two traces: left, then right.
    LeftRight,
    /// Two traces: mid `(L + R) / 2`, then side `(L − R) / 2`.
    MidSide,
}

impl ChannelView {
    /// Number of traces this view draws.
    pub fn traces(self) -> usize {
        match self {
            ChannelView::Mono => 1,
            ChannelView::LeftRight | ChannelView::MidSide => 2,
        }
    }

    /// Splits one stereo frame into this view's traces (the second is 0 for Mono).
    pub fn split(self, left: f32, right: f32) -> [f32; 2] {
        match self {
            ChannelView::Mono => [(left + right) * 0.5, 0.0],
            ChannelView::LeftRight => [left, right],
            ChannelView::MidSide => [(left + right) * 0.5, (left - right) * 0.5],
        }
    }
}
