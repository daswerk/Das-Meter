//! The Send Plugin's side of the table.

use std::hash::{BuildHasher, Hasher};
use std::io;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Relaxed, Release};
use std::sync::atomic::{AtomicU64, fence};
use std::time::Instant;

use crate::layout::*;
use crate::process::process_alive;
use crate::table::{Details, Table};
use crate::{GONE_AFTER, TABLE_NAME};

/// A claimed slot, used from the Send Plugin's main thread.
///
/// Releases the slot when dropped. Drop it only after the audio thread has stopped
/// pushing (in CLAP, after `deactivate`).
pub struct Writer {
    table: Table,
    index: usize,
    generation: u32,
    id: u64,
    app_heartbeat: u64,
    app_heartbeat_at: Option<Instant>,
}

/// The result of [`Writer::claim`].
pub struct Claimed {
    pub writer: Writer,
    /// The ID the slot was claimed under. Store it in plugin state.
    pub id: u64,
    /// The wanted ID was already live in another running Send Plugin (a duplicated
    /// track), so `id` is a fresh one. If the name was an animal name, pick a fresh animal.
    pub id_was_taken: bool,
}

/// Why a slot couldn't be claimed.
fn all_slots_taken() -> io::Error {
    io::Error::other(format!("all {SLOT_COUNT} Send Plugin slots are taken"))
}

impl Writer {
    /// Opens (or creates) the table and claims a slot.
    ///
    /// `wanted_id` is the ID from plugin state, or `None` for a new Send Plugin.
    /// Call this from the main thread: it may make syscalls and allocate.
    pub fn claim(wanted_id: Option<u64>, details: &Details) -> io::Result<Claimed> {
        Self::claim_in(TABLE_NAME, wanted_id, details, std::process::id())
    }

    /// [`Writer::claim`] on a table with another name, for tests and benchmarks.
    pub fn claim_named(
        table_name: &str,
        wanted_id: Option<u64>,
        details: &Details,
    ) -> io::Result<Claimed> {
        Self::claim_in(table_name, wanted_id, details, std::process::id())
    }

    pub(crate) fn claim_in(
        table_name: &str,
        wanted_id: Option<u64>,
        details: &Details,
        host_pid: u32,
    ) -> io::Result<Claimed> {
        let table = Table::open(table_name)?;

        // A live slot with our ID in a running process is a duplicate; one whose
        // process died (a crashed DAW) is ours from before, so free it.
        let mut id_was_taken = false;
        if let Some(wanted) = wanted_id {
            for (_, slot) in table.slots() {
                let state = slot.state.load(Acquire);
                if state == SLOT_FREE || slot.id.load(Relaxed) != wanted {
                    continue;
                }
                if process_alive(slot.host_pid.load(Relaxed)) {
                    id_was_taken = true;
                } else {
                    let _ = slot
                        .state
                        .compare_exchange(state, SLOT_FREE, AcqRel, Relaxed);
                }
            }
        }
        let id = match wanted_id {
            Some(id) if !id_was_taken && id != 0 => id,
            _ => fresh_id(&table),
        };

        let index = claim_free_slot(&table)
            .or_else(|| reclaim_dead_slot(&table))
            .ok_or_else(all_slots_taken)?;
        let slot = table.slot(index);

        let mut generation = slot.generation.load(Relaxed).wrapping_add(1);
        if generation == 0 {
            generation = 1; // 0 means "not listened to"
        }
        slot.generation.store(generation, Relaxed);
        slot.heartbeat.store(0, Relaxed);
        slot.processed.store(0, Relaxed);
        slot.reserved.store(0, Relaxed);
        slot.published.store(0, Relaxed);
        slot.host_pid.store(host_pid, Relaxed);
        slot.id.store(id, Relaxed);
        slot.write_details(details);
        slot.state.store(SLOT_LIVE, Release);

        let writer = Writer {
            table,
            index,
            generation,
            id,
            app_heartbeat: 0,
            app_heartbeat_at: None,
        };
        Ok(Claimed {
            writer,
            id,
            id_was_taken,
        })
    }

    /// The slot's ID.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Changes the name, colour, mono flag or sample rate. The app sees it on its next poll.
    pub fn set_details(&self, details: &Details) {
        if self.owns_slot() {
            self.table.slot(self.index).write_details(details);
        }
    }

    /// Call about every [`HEARTBEAT_INTERVAL`](crate::HEARTBEAT_INTERVAL) from a main-thread timer.
    pub fn heartbeat(&self) {
        if self.owns_slot() {
            self.table.slot(self.index).heartbeat.fetch_add(1, Relaxed);
        }
    }

    /// A handle that bumps this slot's heartbeat from another thread, for hosts that
    /// give the plugin no main-thread timer.
    pub fn heartbeat_handle(&self) -> Heartbeat {
        Heartbeat {
            table: self.table.clone(),
            index: self.index,
            generation: self.generation,
        }
    }

    /// Whether the app has bumped its heartbeat within [`GONE_AFTER`]. Poll it from the same timer.
    pub fn app_running(&mut self, now: Instant) -> bool {
        let beat = self.table.header().app_heartbeat.load(Relaxed);
        if beat == 0 {
            return false;
        }
        if beat != self.app_heartbeat || self.app_heartbeat_at.is_none() {
            self.app_heartbeat = beat;
            self.app_heartbeat_at = Some(now);
        }
        self.app_heartbeat_at
            .is_some_and(|at| now.saturating_duration_since(at) < GONE_AFTER)
    }

    /// A handle for the audio thread. Create it on the main thread (it clones an `Arc`),
    /// and make only one per claim.
    pub fn audio(&self) -> AudioWriter {
        let slot = self.table.slot(self.index);
        AudioWriter {
            table: self.table.clone(),
            index: self.index,
            generation: self.generation,
            position: slot.published.load(Relaxed),
            processed: slot.processed.load(Relaxed),
        }
    }

    fn owns_slot(&self) -> bool {
        let slot = self.table.slot(self.index);
        slot.generation.load(Relaxed) == self.generation && slot.state.load(Relaxed) == SLOT_LIVE
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        let slot = self.table.slot(self.index);
        if slot.generation.load(Relaxed) == self.generation {
            let _ = slot
                .state
                .compare_exchange(SLOT_LIVE, SLOT_FREE, AcqRel, Relaxed);
        }
    }
}

/// Bumps a claimed slot's heartbeat from any thread. See [`Writer::heartbeat_handle`].
pub struct Heartbeat {
    table: Table,
    index: usize,
    generation: u32,
}

impl Heartbeat {
    /// Bumps the heartbeat. Returns `false` once the slot has been released, so a
    /// heartbeat thread knows to stop.
    pub fn beat(&self) -> bool {
        let slot = self.table.slot(self.index);
        if slot.generation.load(Relaxed) != self.generation || slot.state.load(Relaxed) != SLOT_LIVE
        {
            return false;
        }
        slot.heartbeat.fetch_add(1, Relaxed);
        true
    }
}

/// Pushes audio into a claimed slot from the audio thread.
///
/// [`AudioWriter::push`] makes no allocations, takes no locks and makes no
/// syscalls: it does plain atomic loads and stores into the mapped table and
/// never waits for the reader. Drop the handle on the main thread; if it holds
/// the last reference to the table, dropping it unmaps the memory.
pub struct AudioWriter {
    table: Table,
    index: usize,
    generation: u32,
    position: u64,
    processed: u64,
}

impl AudioWriter {
    /// Pushes one block of stereo audio. For a mono track pass the same slice twice.
    ///
    /// Counts the frames even when nobody listens, so the app can tell a playing
    /// track from an idle one, but copies samples only while the app listens. The
    /// oldest audio is always overwritten.
    pub fn push(&mut self, left: &[f32], right: &[f32]) {
        let frames = left.len().min(right.len());
        let slot = self.table.slot(self.index);
        if slot.generation.load(Relaxed) != self.generation {
            return; // released and maybe reused by another Send Plugin
        }

        self.processed = self.processed.wrapping_add(frames as u64);
        slot.processed.store(self.processed, Relaxed);
        if slot.listened.load(Relaxed) != self.generation {
            return;
        }

        // Announce which frames are about to be overwritten before touching them,
        // so a reader copying concurrently can tell which of its frames are torn.
        let end = self.position + frames as u64;
        slot.reserved.store(end, Relaxed);
        fence(Release);

        let skip = frames.saturating_sub(RING_FRAMES);
        let mask = RING_FRAMES - 1;
        for (offset, (l, r)) in left[skip..frames]
            .iter()
            .zip(&right[skip..frames])
            .enumerate()
        {
            let at = ((self.position as usize).wrapping_add(skip + offset) & mask) * 2;
            slot.ring[at].store(l.to_bits(), Relaxed);
            slot.ring[at + 1].store(r.to_bits(), Relaxed);
        }

        slot.published.store(end, Release);
        self.position = end;
    }
}

fn claim_free_slot(table: &Table) -> Option<usize> {
    table.slots().find_map(|(index, slot)| {
        slot.state
            .compare_exchange(SLOT_FREE, SLOT_CLAIMING, AcqRel, Relaxed)
            .is_ok()
            .then_some(index)
    })
}

/// Takes over a slot whose host process has exited without releasing it.
fn reclaim_dead_slot(table: &Table) -> Option<usize> {
    table.slots().find_map(|(index, slot)| {
        let state = slot.state.load(Acquire);
        if state == SLOT_FREE || process_alive(slot.host_pid.load(Relaxed)) {
            return None;
        }
        slot.state
            .compare_exchange(state, SLOT_CLAIMING, AcqRel, Relaxed)
            .is_ok()
            .then_some(index)
    })
}

/// A random, non-zero ID not used by any slot in the table.
fn fresh_id(table: &Table) -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    loop {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(COUNTER.fetch_add(1, Relaxed));
        hasher.write_u32(std::process::id());
        hasher.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        );
        let id = hasher.finish();
        let in_use = table
            .slots()
            .any(|(_, slot)| slot.state.load(Relaxed) != SLOT_FREE && slot.id.load(Relaxed) == id);
        if id != 0 && !in_use {
            return id;
        }
    }
}
