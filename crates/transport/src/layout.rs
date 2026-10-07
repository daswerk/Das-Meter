//! The byte layout of the `dasmeter.v2` table, and of `dasmeter.v1` before it.
//!
//! Every field is an atomic, so any bit pattern another process leaves behind is
//! a valid value to read; garbage is caught by validation, never by undefined
//! behaviour. Changing anything here means a new layout and a new table name.
//!
//! v2 is v1 with one [`TimingBlock`] per slot after the slots: where the
//! Send Plugin's DAW is (tempo, song position, time signature, playing).

use std::mem::size_of;
use std::sync::atomic::{AtomicU32, AtomicU64};

/// `"DASMETER"` as a little-endian `u64`.
pub(crate) const MAGIC: u64 = u64::from_le_bytes(*b"DASMETER");

/// The layout versions, also spelled out in the tables' names. Send Plugins
/// write v2; the app reads v2 and v1, for older Send Plugins in saved projects.
pub(crate) const V1: u32 = 1;
pub(crate) const V2: u32 = 2;

/// Number of Send Plugin slots in the table.
pub const SLOT_COUNT: usize = 64;

/// Stereo frames in each slot's ring. A power of two, so positions wrap with a mask.
pub const RING_FRAMES: usize = 16_384;

/// Bytes of UTF-8 a Send Plugin name can hold. Longer names are cut at a character boundary.
pub const NAME_CAPACITY: usize = 64;

/// Header `init` values.
pub(crate) const INIT_EMPTY: u32 = 0;
pub(crate) const INIT_BUSY: u32 = 1;
pub(crate) const INIT_READY: u32 = 2;

/// Slot `state` values.
pub(crate) const SLOT_FREE: u32 = 0;
pub(crate) const SLOT_CLAIMING: u32 = 1;
pub(crate) const SLOT_LIVE: u32 = 2;

/// Slot `flags` bits.
pub(crate) const FLAG_MONO: u32 = 1;

#[repr(C, align(64))]
pub(crate) struct Header {
    pub magic: AtomicU64,
    pub init: AtomicU32,
    pub layout_version: AtomicU32,
    pub slot_count: AtomicU32,
    pub ring_frames: AtomicU32,
    pub slot_size: AtomicU32,
    pub header_size: AtomicU32,
    /// Bumped by the app about every 250 ms while it runs.
    pub app_heartbeat: AtomicU64,
}

#[repr(C, align(64))]
pub(crate) struct Slot {
    /// `SLOT_FREE`, `SLOT_CLAIMING` or `SLOT_LIVE`.
    pub state: AtomicU32,
    /// Bumped on every claim, so a reused slot is never mistaken for the old one.
    pub generation: AtomicU32,
    /// The generation the app listens to. The writer sends audio only while it matches its own.
    pub listened: AtomicU32,
    /// Seqlock over the details below: odd while the writer changes them.
    pub details_seq: AtomicU32,
    pub id: AtomicU64,
    pub host_pid: AtomicU32,
    pub sample_rate: AtomicU32,
    /// `0x00RRGGBB`.
    pub colour: AtomicU32,
    pub flags: AtomicU32,
    pub name_len: AtomicU32,
    pub _reserved: AtomicU32,
    pub name: [AtomicU64; NAME_CAPACITY / 8],
    /// Bumped by the Send Plugin's main thread about every 250 ms.
    pub heartbeat: AtomicU64,
    /// Frames the audio thread has processed, listened to or not. Still means idle.
    pub processed: AtomicU64,
    /// End of the frames the writer is about to overwrite (set before the samples).
    pub reserved: AtomicU64,
    /// End of the frames that are fully written (set after the samples).
    pub published: AtomicU64,
    /// Interleaved stereo `f32` samples, stored as bits.
    pub ring: [AtomicU32; RING_FRAMES * 2],
}

/// Timing `flags` bits.
pub(crate) const TIMING_SET: u32 = 1;
pub(crate) const TIMING_PLAYING: u32 = 2;

/// Where a slot's Send Plugin's DAW is (v2 only), written by its audio thread
/// before each block under a seqlock.
#[repr(C, align(64))]
pub(crate) struct TimingBlock {
    /// Seqlock: odd while the audio thread writes.
    pub seq: AtomicU32,
    /// `TIMING_SET` once the DAW said where it is; `TIMING_PLAYING` while it plays.
    pub flags: AtomicU32,
    /// The ring position (as `published` counts) the song position is for.
    pub frame: AtomicU64,
    /// `f64` bits: the song position in quarter notes.
    pub beats: AtomicU64,
    /// `f64` bits: quarter notes per minute.
    pub tempo: AtomicU64,
    /// `f64` bits: where the current bar began, in quarter notes.
    pub bar_start: AtomicU64,
    /// Beats per bar in the high 16 bits, the beat's note value in the low.
    pub signature: AtomicU32,
    pub _reserved: AtomicU32,
}

pub(crate) const HEADER_SIZE: usize = size_of::<Header>();
pub(crate) const SLOT_SIZE: usize = size_of::<Slot>();
pub(crate) const TIMING_SIZE: usize = size_of::<TimingBlock>();
/// Where the timing blocks start in a v2 table.
pub(crate) const TIMING_OFFSET: usize = HEADER_SIZE + SLOT_COUNT * SLOT_SIZE;

/// Page granularity used for the table size: the largest page size among our targets (Apple silicon).
const PAGE: usize = 16_384;

/// Bytes a table of `version` occupies, rounded up to whole pages.
pub(crate) const fn table_size(version: u32) -> usize {
    let timing = if version >= V2 {
        SLOT_COUNT * TIMING_SIZE
    } else {
        0
    };
    (TIMING_OFFSET + timing).div_ceil(PAGE) * PAGE
}

const _: () = assert!(RING_FRAMES.is_power_of_two());
const _: () = assert!(NAME_CAPACITY % 8 == 0);
const _: () = assert!(HEADER_SIZE % 64 == 0);
const _: () = assert!(SLOT_SIZE % 64 == 0);
const _: () = assert!(TIMING_SIZE % 64 == 0);
