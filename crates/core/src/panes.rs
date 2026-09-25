//! Window mode: one ordinary window split into panes like a tiling editor,
//! one Meter per pane, as a split tree.

use crate::layout::Rect;
use crate::scene::Frame;

/// How a split divides its area.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Direction {
    /// Two panes next to each other, a vertical divider between them.
    SideBySide,
    /// Two panes one above the other, a horizontal divider between them.
    Stacked,
}

/// Where a split sits in the tree: the turns from the root, first (0) or
/// second (1), lowest bit first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SplitId {
    pub bits: u32,
    pub depth: u8,
}

impl SplitId {
    const ROOT: SplitId = SplitId { bits: 0, depth: 0 };

    fn child(self, second: bool) -> SplitId {
        SplitId {
            bits: self.bits | (u32::from(second) << self.depth),
            depth: self.depth + 1,
        }
    }
}

/// The deepest a split tree goes (a [`SplitId`] holds 32 turns).
const MAX_DEPTH: u8 = 31;

/// The smallest share a pane keeps of its split.
pub const MIN_RATIO: f32 = 0.05;

/// A split tree: a pane showing a Meter, or two subtrees.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Node {
    Pane(usize),
    Split {
        direction: Direction,
        /// The first subtree's share, 0–1.
        ratio: f32,
        first: Box<Node>,
        second: Box<Node>,
    },
}

/// A split's divider, for the shell to hit-test and drag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Divider {
    pub split: SplitId,
    pub direction: Direction,
    /// The split's whole area, as fractions of the window.
    pub area: Frame,
    pub ratio: f32,
}

impl Node {
    fn split(direction: Direction, ratio: f32, first: Node, second: Node) -> Node {
        Node::Split {
            direction,
            ratio,
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    /// The Meters in the tree's panes, first to last.
    pub fn meters(&self) -> Vec<usize> {
        let mut out = Vec::new();
        self.collect(&mut out);
        out
    }

    fn collect(&self, out: &mut Vec<usize>) {
        match self {
            Node::Pane(meter) => out.push(*meter),
            Node::Split { first, second, .. } => {
                first.collect(out);
                second.collect(out);
            }
        }
    }

    /// Each pane's Meter and frame, and each split's divider, inside `area`.
    pub fn place(&self, area: Frame) -> (Vec<(usize, Frame)>, Vec<Divider>) {
        let mut panes = Vec::new();
        let mut dividers = Vec::new();
        self.place_into(area, SplitId::ROOT, &mut panes, &mut dividers);
        (panes, dividers)
    }

    fn place_into(
        &self,
        area: Frame,
        id: SplitId,
        panes: &mut Vec<(usize, Frame)>,
        dividers: &mut Vec<Divider>,
    ) {
        match self {
            Node::Pane(meter) => panes.push((*meter, area)),
            Node::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let (a, b) = match direction {
                    Direction::SideBySide => (
                        Frame {
                            width: area.width * ratio,
                            ..area
                        },
                        Frame {
                            x: area.x + area.width * ratio,
                            width: area.width * (1.0 - ratio),
                            ..area
                        },
                    ),
                    Direction::Stacked => (
                        Frame {
                            height: area.height * ratio,
                            ..area
                        },
                        Frame {
                            y: area.y + area.height * ratio,
                            height: area.height * (1.0 - ratio),
                            ..area
                        },
                    ),
                };
                dividers.push(Divider {
                    split: id,
                    direction: *direction,
                    area,
                    ratio: *ratio,
                });
                first.place_into(a, id.child(false), panes, dividers);
                second.place_into(b, id.child(true), panes, dividers);
            }
        }
    }

    fn depth_of(&self, meter: usize) -> Option<u8> {
        match self {
            Node::Pane(m) => (*m == meter).then_some(0),
            Node::Split { first, second, .. } => first
                .depth_of(meter)
                .or_else(|| second.depth_of(meter))
                .map(|d| d + 1),
        }
    }

    /// Splits the pane showing `meter`: it keeps the first half, `new` gets the second.
    pub fn split_pane(&mut self, meter: usize, direction: Direction, new: usize) -> bool {
        if self.depth_of(meter).is_none_or(|d| d >= MAX_DEPTH) {
            return false;
        }
        self.split_inner(meter, direction, new)
    }

    fn split_inner(&mut self, meter: usize, direction: Direction, new: usize) -> bool {
        match self {
            Node::Pane(m) if *m == meter => {
                *self = Node::split(direction, 0.5, Node::Pane(meter), Node::Pane(new));
                true
            }
            Node::Pane(_) => false,
            Node::Split { first, second, .. } => {
                first.split_inner(meter, direction, new)
                    || second.split_inner(meter, direction, new)
            }
        }
    }

    /// Closes the pane showing `meter`; its sibling takes the split's place.
    /// The last pane stays.
    pub fn close_pane(&mut self, meter: usize) -> bool {
        match self {
            Node::Pane(_) => false,
            Node::Split { first, second, .. } => {
                let keep = if **first == Node::Pane(meter) {
                    Some(std::mem::replace(second.as_mut(), Node::Pane(0)))
                } else if **second == Node::Pane(meter) {
                    Some(std::mem::replace(first.as_mut(), Node::Pane(0)))
                } else {
                    None
                };
                match keep {
                    Some(sibling) => {
                        *self = sibling;
                        true
                    }
                    None => first.close_pane(meter) || second.close_pane(meter),
                }
            }
        }
    }

    /// Sets the ratio of the split at `id`, keeping each side at least [`MIN_RATIO`].
    pub fn set_ratio(&mut self, id: SplitId, ratio: f32) -> bool {
        if ratio.is_nan() {
            return false;
        }
        let mut node = self;
        for turn in 0..id.depth {
            let Node::Split { first, second, .. } = node else {
                return false;
            };
            node = if id.bits >> turn & 1 == 1 {
                second
            } else {
                first
            };
        }
        let Node::Split { ratio: r, .. } = node else {
            return false;
        };
        let ratio = ratio.clamp(MIN_RATIO, 1.0 - MIN_RATIO);
        if *r == ratio {
            return false;
        }
        *r = ratio;
        true
    }
}

/// Window mode's layout.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WindowLayout {
    /// Where the window is, once placed or moved: relative to its display's
    /// top-left corner.
    pub frame: Option<Rect>,
    /// The display it was placed on; `None` for the main display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<crate::displays::DisplayRef>,
    pub on_top: bool,
    pub tree: Node,
}

impl WindowLayout {
    /// The spec's Mixing layout: the Spectrum on top; Waveform, Loudness Meter
    /// and Stereometer below. Meters are the default row's: Waveform 0,
    /// Spectrum 1, Stereometer 2, Loudness Meter 3.
    pub fn mixing() -> WindowLayout {
        let below = Node::split(
            Direction::SideBySide,
            1.0 / 3.0,
            Node::Pane(0),
            Node::split(Direction::SideBySide, 0.5, Node::Pane(3), Node::Pane(2)),
        );
        WindowLayout {
            frame: None,
            display: None,
            on_top: false,
            tree: Node::split(Direction::Stacked, 0.5, Node::Pane(1), below),
        }
    }
}
