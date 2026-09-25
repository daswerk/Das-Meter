//! The shared-memory table that carries audio from Send Plugins to the app.
//!
//! See ADR 0003 (`docs/adr/0003-send-plugin-shared-memory-table.md`).
//!
//! A Send Plugin claims a slot with [`Writer::claim`], pushes audio from its
//! audio thread through an [`AudioWriter`] and bumps [`Writer::heartbeat`] from
//! a main-thread timer. The app opens a [`Reader`], lists slots with their
//! [`SlotState`], sets the listened-to flag, and reads the newest frames.
//! Whichever side comes first creates the table.

use std::time::Duration;

mod layout;
mod process;
mod reader;
mod shm;
mod table;
mod writer;

pub use layout::{NAME_CAPACITY, RING_FRAMES, SLOT_COUNT};
pub use reader::{Read, Reader, SlotInfo, SlotRef, SlotState};
pub use table::Details;
pub use writer::{AudioWriter, Claimed, Heartbeat, Writer};

/// Name of the shared-memory table on macOS. The layout version is part of the name.
pub const TABLE_NAME: &str = "dasmeter.v1";

/// Name of the shared-memory table on Windows, in the session namespace.
pub const TABLE_NAME_WINDOWS: &str = r"Local\dasmeter.v1";

/// How often both sides bump their heartbeat.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_millis(250);

/// A Send Plugin (or the app) without a heartbeat for this long is gone.
pub const GONE_AFTER: Duration = Duration::from_secs(2);

/// A Send Plugin whose frame counter hasn't moved for this long is idle.
pub const IDLE_AFTER: Duration = Duration::from_millis(500);

/// The platform's name for a table called `name`.
fn platform_name(name: &str) -> String {
    if cfg!(windows) {
        format!(r"Local\{name}")
    } else {
        name.to_owned()
    }
}

/// Removes a table's name so the next opener creates a fresh one, for tests and
/// benchmarks. Open handles keep working. Windows removes a table with its last handle.
pub fn remove_table(name: &str) {
    shm::unlink(&platform_name(name));
}

#[cfg(test)]
mod tests;
