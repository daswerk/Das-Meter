//! The app's side of the table.

use std::io;
use std::sync::atomic::Ordering::{Acquire, Relaxed};
use std::sync::atomic::fence;
use std::time::Instant;

use crate::layout::*;
use crate::process::ProcessWatch;
use crate::table::{Details, Table};
use crate::{GONE_AFTER, IDLE_AFTER, TABLE_NAME};

/// One claim of one slot. A slot that is released and claimed again gets a new `SlotRef`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SlotRef {
    index: usize,
    generation: u32,
}

/// How a listed Send Plugin is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotState {
    /// Audio is flowing.
    Live,
    /// The Send Plugin is there but isn't processing audio (transport stopped, bypassed).
    Idle,
    /// Its host process exited or its heartbeat stopped for [`GONE_AFTER`].
    /// A Send Plugin that releases its slot is simply no longer listed.
    Gone,
}

/// A Send Plugin in the table, as the app sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotInfo {
    pub slot: SlotRef,
    pub id: u64,
    pub details: Details,
    pub host_pid: u32,
    pub state: SlotState,
}

/// The outcome of [`Reader::read`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Read {
    /// Stereo frames written to the start of the output, oldest first.
    pub frames: usize,
    /// Pass this as `since` next time.
    pub next: u64,
    /// Frames between `since` and the first frame returned that were lost
    /// (overwritten, or more than the output could hold).
    pub skipped: u64,
}

#[derive(Clone, Copy)]
struct Track {
    generation: u32,
    heartbeat: u64,
    heartbeat_at: Instant,
    processed: u64,
    processed_at: Instant,
}

/// Lists Send Plugins, chooses which to listen to, and reads their audio.
pub struct Reader {
    table: Table,
    tracks: [Option<Track>; SLOT_COUNT],
    processes: ProcessWatch,
}

impl Reader {
    /// Opens (or creates) the table.
    pub fn open() -> io::Result<Reader> {
        Self::open_named(TABLE_NAME)
    }

    /// [`Reader::open`] on a table with another name, for tests and benchmarks.
    pub fn open_named(name: &str) -> io::Result<Reader> {
        Ok(Reader {
            table: Table::open(name)?,
            tracks: [None; SLOT_COUNT],
            processes: ProcessWatch::default(),
        })
    }

    /// Tells Send Plugins the app is running. Call about every
    /// [`HEARTBEAT_INTERVAL`](crate::HEARTBEAT_INTERVAL).
    pub fn heartbeat(&self) {
        self.table.header().app_heartbeat.fetch_add(1, Relaxed);
    }

    /// Every claimed slot with its details and state. Slots with malformed contents are left out.
    ///
    /// States are judged by how the heartbeat and frame counter moved between calls,
    /// so call this regularly (every few hundred milliseconds) with the current time.
    pub fn slots(&mut self, now: Instant) -> Vec<SlotInfo> {
        let mut listed = Vec::new();
        for index in 0..SLOT_COUNT {
            let Some((slot_ref, id, details, host_pid, heartbeat, processed)) =
                self.snapshot(index)
            else {
                self.tracks[index] = None;
                continue;
            };
            let track = match &mut self.tracks[index] {
                Some(track) if track.generation == slot_ref.generation => {
                    if track.heartbeat != heartbeat {
                        track.heartbeat = heartbeat;
                        track.heartbeat_at = now;
                    }
                    if track.processed != processed {
                        track.processed = processed;
                        track.processed_at = now;
                    }
                    *track
                }
                entry => *entry.insert(Track {
                    generation: slot_ref.generation,
                    heartbeat,
                    heartbeat_at: now,
                    processed,
                    processed_at: now,
                }),
            };
            let state = if !self.processes.is_alive(host_pid)
                || now.saturating_duration_since(track.heartbeat_at) >= GONE_AFTER
            {
                SlotState::Gone
            } else if now.saturating_duration_since(track.processed_at) >= IDLE_AFTER {
                SlotState::Idle
            } else {
                SlotState::Live
            };
            listed.push(SlotInfo {
                slot: slot_ref,
                id,
                details,
                host_pid,
                state,
            });
        }
        let pids: Vec<u32> = listed.iter().map(|info| info.host_pid).collect();
        self.processes.retain(&pids);
        listed
    }

    /// A consistent view of one live slot.
    fn snapshot(&self, index: usize) -> Option<(SlotRef, u64, Details, u32, u64, u64)> {
        let slot = self.table.slot(index);
        if slot.state.load(Acquire) != SLOT_LIVE {
            return None;
        }
        let generation = slot.generation.load(Acquire);
        let (id, details) = slot.read_details()?;
        let host_pid = slot.host_pid.load(Relaxed);
        let heartbeat = slot.heartbeat.load(Relaxed);
        let processed = slot.processed.load(Relaxed);
        fence(Acquire);
        if slot.state.load(Relaxed) != SLOT_LIVE || slot.generation.load(Relaxed) != generation {
            return None;
        }
        let slot_ref = SlotRef { index, generation };
        Some((slot_ref, id, details, host_pid, heartbeat, processed))
    }

    /// Starts or stops the audio from one Send Plugin. It sends nothing while not listened to.
    pub fn set_listened(&self, slot_ref: SlotRef, listened: bool) {
        let slot = self.table.slot(slot_ref.index);
        if listened {
            if self.is_current(slot_ref) {
                slot.listened.store(slot_ref.generation, Relaxed);
            }
        } else {
            let _ = slot
                .listened
                .compare_exchange(slot_ref.generation, 0, Relaxed, Relaxed);
        }
    }

    /// The position of the newest frame, to start reading from when listening begins
    /// (so audio left over from an earlier listen isn't shown). `None` if the slot was released.
    pub fn cursor(&self, slot_ref: SlotRef) -> Option<u64> {
        let slot = self.table.slot(slot_ref.index);
        let position = slot.published.load(Acquire);
        self.is_current(slot_ref).then_some(position)
    }

    /// Copies the frames published after `since` into `out` (interleaved stereo),
    /// newest last. A reader that has fallen behind jumps to the newest audio
    /// that fits in `out`. `None` if the slot was released or reused.
    pub fn read(&self, slot_ref: SlotRef, since: u64, out: &mut [f32]) -> Option<Read> {
        if !self.is_current(slot_ref) {
            return None;
        }
        let slot = self.table.slot(slot_ref.index);
        let end = slot.published.load(Acquire);
        let since = since.min(end);
        let capacity = (out.len() / 2).min(RING_FRAMES) as u64;
        let start = since
            .max(end.saturating_sub(RING_FRAMES as u64))
            .max(end - capacity.min(end));

        let mask = RING_FRAMES - 1;
        for (frame, position) in (start..end).enumerate() {
            let at = (position as usize & mask) * 2;
            out[frame * 2] = f32::from_bits(slot.ring[at].load(Relaxed));
            out[frame * 2 + 1] = f32::from_bits(slot.ring[at + 1].load(Relaxed));
        }

        // Frames older than `reserved - RING_FRAMES` may have been overwritten while we copied.
        fence(Acquire);
        let reserved = slot.reserved.load(Relaxed);
        let first_intact = reserved
            .saturating_sub(RING_FRAMES as u64)
            .clamp(start, end);
        let torn = (first_intact - start) as usize;
        let frames = (end - first_intact) as usize;
        if torn > 0 {
            out.copy_within(torn * 2..(torn + frames) * 2, 0);
        }
        if !self.is_current(slot_ref) {
            return None;
        }
        Some(Read {
            frames,
            next: end,
            skipped: first_intact - since,
        })
    }

    fn is_current(&self, slot_ref: SlotRef) -> bool {
        let slot = self.table.slot(slot_ref.index);
        slot.state.load(Acquire) == SLOT_LIVE
            && slot.generation.load(Acquire) == slot_ref.generation
    }
}
