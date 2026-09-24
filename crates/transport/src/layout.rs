//! The byte layout of the `dasmeter.v1` table.
//!
//! Every field is an atomic, so any bit pattern another process leaves behind is
//! a valid value to read; garbage is caught by validation, never by undefined
//! behaviour. Changing anything here means a new layout and a new table name.

use std::mem::size_of;
use std::sync::atomic::{AtomicU32, AtomicU64};

/// `"DASMETER"` as a little-endian `u64`.
pub(crate) const MAGIC: u64 = u64::from_le_bytes(*b"DASMETER");

/// The layout version, also spelled out in the table's name.
pub(crate) const LAYOUT_VERSION: u32 = 1;

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

pub(crate) const HEADER_SIZE: usize = size_of::<Header>();
pub(crate) const SLOT_SIZE: usize = size_of::<Slot>();

/// Page granularity used for the table size: the largest page size among our targets (Apple silicon).
const PAGE: usize = 16_384;

/// Bytes the table occupies, rounded up to whole pages.
pub(crate) const TABLE_SIZE: usize = (HEADER_SIZE + SLOT_COUNT * SLOT_SIZE).div_ceil(PAGE) * PAGE;

const _: () = assert!(RING_FRAMES.is_power_of_two());
const _: () = assert!(NAME_CAPACITY % 8 == 0);
const _: () = assert!(HEADER_SIZE % 64 == 0);
const _: () = assert!(SLOT_SIZE % 64 == 0);
