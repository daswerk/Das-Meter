//! In-process tests: a writer and a reader open the same table in one process.

use std::io;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering::Relaxed;
use std::time::{Duration, Instant};

use super::*;
use crate::layout::SLOT_LIVE;
use crate::table::Table;

/// A table with a unique name, removed again when the test ends.
struct TestTable(String);

impl TestTable {
    fn new() -> TestTable {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let name = format!("dmt{}.{}", std::process::id(), NEXT.fetch_add(1, Relaxed));
        assert!(platform_name(&name).len() <= 31);
        TestTable(name)
    }

    fn reader(&self) -> Reader {
        Reader::open_named(&self.0).unwrap()
    }

    fn claim(&self, wanted_id: Option<u64>) -> Claimed {
        self.claim_as(wanted_id, std::process::id())
    }

    fn claim_as(&self, wanted_id: Option<u64>, host_pid: u32) -> Claimed {
        Writer::claim_in(&self.0, wanted_id, &details("Otter"), host_pid).unwrap()
    }

    fn raw(&self) -> Table {
        Table::open(&self.0).unwrap()
    }
}

impl Drop for TestTable {
    fn drop(&mut self) {
        shm::unlink(&platform_name(&self.0));
    }
}

fn details(name: &str) -> Details {
    Details {
        name: name.to_owned(),
        colour: 0x33_99_ff,
        mono: false,
        sample_rate: 48_000,
    }
}

/// Pushes frames `from..to`, each sample holding its own frame number (left) and its negative (right).
fn push_counting(audio: &mut AudioWriter, from: u64, to: u64, block: usize) {
    let mut frame = from;
    while frame < to {
        let n = block.min((to - frame) as usize);
        let left: Vec<f32> = (0..n).map(|i| (frame + i as u64) as f32).collect();
        let right: Vec<f32> = left.iter().map(|x| -x).collect();
        audio.push(&left, &right);
        frame += n as u64;
    }
}

fn only_slot(reader: &mut Reader, now: Instant) -> SlotInfo {
    let slots = reader.slots(now);
    assert_eq!(slots.len(), 1, "{slots:?}");
    slots.into_iter().next().unwrap()
}

/// The PID of a process that has already exited.
fn exited_pid() -> u32 {
    let mut child = if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/C", "exit"])
            .spawn()
    } else {
        std::process::Command::new("true").spawn()
    }
    .unwrap();
    let pid = child.id();
    child.wait().unwrap();
    pid
}

#[test]
fn table_names_fit_the_macos_sandbox_limit() {
    // A sandboxed macOS process can only open names of 31 characters or less.
    assert!(TABLE_NAME.len() <= 31);
    assert!(TABLE_NAME_WINDOWS.len() <= 31);
}

#[test]
fn windows_name_is_the_macos_name_in_the_session_namespace() {
    assert_eq!(TABLE_NAME_WINDOWS, format!(r"Local\{TABLE_NAME}"));
    if cfg!(windows) {
        assert_eq!(platform_name(TABLE_NAME), TABLE_NAME_WINDOWS);
    }
}

#[test]
fn a_claimed_slot_is_listed_with_its_details_until_released() {
    let table = TestTable::new();
    let mut reader = table.reader();
    let now = Instant::now();
    assert!(reader.slots(now).is_empty());

    let claimed = table.claim(Some(42));
    assert_eq!(claimed.id, 42);
    assert!(!claimed.id_was_taken);

    let info = only_slot(&mut reader, now);
    assert_eq!(info.id, 42);
    assert_eq!(info.details, details("Otter"));
    assert_eq!(info.host_pid, std::process::id());
    assert_eq!(info.state, SlotState::Live);

    claimed.writer.set_details(&Details {
        mono: true,
        ..details("Kick")
    });
    let info = only_slot(&mut reader, now);
    assert_eq!(info.details.name, "Kick");
    assert!(info.details.mono);

    drop(claimed);
    assert!(reader.slots(now).is_empty());
    assert_eq!(reader.cursor(info.slot), None);
    assert_eq!(reader.read(info.slot, 0, &mut [0.0; 64]), None);
}

#[test]
fn a_reused_slot_is_a_different_slot_ref() {
    let table = TestTable::new();
    let mut reader = table.reader();
    let now = Instant::now();

    let first = table.claim(None);
    let first_ref = only_slot(&mut reader, now).slot;
    drop(first);

    let _second = table.claim(None);
    let second_ref = only_slot(&mut reader, now).slot;
    assert_ne!(first_ref, second_ref);

    // Listening to the old claim doesn't reach the new one.
    reader.set_listened(first_ref, true);
    assert_eq!(reader.read(first_ref, 0, &mut [0.0; 64]), None);
}

#[test]
fn a_new_send_plugin_gets_a_fresh_non_zero_id() {
    let table = TestTable::new();
    let a = table.claim(None);
    let b = table.claim(None);
    assert_ne!(a.id, 0);
    assert_ne!(a.id, b.id);
    assert_eq!(a.writer.id(), a.id);
}

#[test]
fn the_ring_wraps_and_overwrites_the_oldest_audio() {
    let table = TestTable::new();
    let mut reader = table.reader();
    let claimed = table.claim(None);
    let slot = only_slot(&mut reader, Instant::now()).slot;
    reader.set_listened(slot, true);

    let mut audio = claimed.writer.audio();
    let total = (RING_FRAMES * 5 / 2) as u64;
    push_counting(&mut audio, 0, total, 64);

    let mut out = vec![0.0; RING_FRAMES * 2];
    let read = reader.read(slot, 0, &mut out).unwrap();
    assert_eq!(read.frames, RING_FRAMES);
    assert_eq!(read.next, total);
    assert_eq!(read.skipped, total - RING_FRAMES as u64);
    let oldest = (total - RING_FRAMES as u64) as f32;
    assert_eq!(&out[..2], &[oldest, -oldest]);
    assert_eq!(
        &out[out.len() - 2..],
        &[(total - 1) as f32, -((total - 1) as f32)]
    );
    for (i, frame) in out.chunks_exact(2).enumerate() {
        assert_eq!(frame[0], oldest + i as f32);
    }
}

#[test]
fn a_block_larger_than_the_ring_keeps_its_newest_frames() {
    let table = TestTable::new();
    let mut reader = table.reader();
    let claimed = table.claim(None);
    let slot = only_slot(&mut reader, Instant::now()).slot;
    reader.set_listened(slot, true);

    let mut audio = claimed.writer.audio();
    let total = (RING_FRAMES + 100) as u64;
    push_counting(&mut audio, 0, total, total as usize);

    let mut out = vec![0.0; RING_FRAMES * 2];
    let read = reader.read(slot, 0, &mut out).unwrap();
    assert_eq!(read.frames, RING_FRAMES);
    assert_eq!(out[0], 100.0);
    assert_eq!(out[out.len() - 2], (total - 1) as f32);
}

#[test]
fn a_lagging_reader_jumps_to_the_newest_audio() {
    let table = TestTable::new();
    let mut reader = table.reader();
    let claimed = table.claim(None);
    let slot = only_slot(&mut reader, Instant::now()).slot;
    reader.set_listened(slot, true);
    let mut audio = claimed.writer.audio();

    push_counting(&mut audio, 0, 5_000, 64);
    let mut out = vec![0.0; 1_000 * 2];
    let read = reader.read(slot, 0, &mut out).unwrap();
    assert_eq!(
        read,
        Read {
            frames: 1_000,
            next: 5_000,
            skipped: 4_000
        }
    );
    assert_eq!(out[0], 4_000.0);

    // Caught up: only the new frames come back.
    push_counting(&mut audio, 5_000, 5_010, 64);
    let read = reader.read(slot, read.next, &mut out).unwrap();
    assert_eq!(
        read,
        Read {
            frames: 10,
            next: 5_010,
            skipped: 0
        }
    );
    assert_eq!(&out[..2], &[5_000.0, -5_000.0]);

    // Nothing new.
    let read = reader.read(slot, read.next, &mut out).unwrap();
    assert_eq!(
        read,
        Read {
            frames: 0,
            next: 5_010,
            skipped: 0
        }
    );
}

#[test]
fn audio_is_written_only_while_listened_to() {
    let table = TestTable::new();
    let mut reader = table.reader();
    let claimed = table.claim(None);
    let t0 = Instant::now();
    let slot = only_slot(&mut reader, t0).slot;
    let mut audio = claimed.writer.audio();
    let mut out = vec![0.0; 4_096];

    // Not listened to: nothing is written, but the frames still count as processed.
    push_counting(&mut audio, 0, 1_000, 64);
    assert_eq!(reader.cursor(slot), Some(0));
    assert_eq!(reader.read(slot, 0, &mut out).unwrap().frames, 0);
    assert_eq!(
        only_slot(&mut reader, t0 + IDLE_AFTER).state,
        SlotState::Live
    );

    reader.set_listened(slot, true);
    let cursor = reader.cursor(slot).unwrap();
    push_counting(&mut audio, 1_000, 1_100, 64);
    let read = reader.read(slot, cursor, &mut out).unwrap();
    assert_eq!(read.frames, 100);
    assert_eq!(out[0], 1_000.0);

    reader.set_listened(slot, false);
    push_counting(&mut audio, 1_100, 1_200, 64);
    assert_eq!(reader.read(slot, read.next, &mut out).unwrap().frames, 0);
}

#[test]
fn a_heartbeat_timeout_means_gone() {
    let table = TestTable::new();
    let mut reader = table.reader();
    let claimed = table.claim(None);
    let mut audio = claimed.writer.audio();
    let t0 = Instant::now();
    let tick = |reader: &mut Reader, audio: &mut AudioWriter, at: Duration| {
        audio.push(&[0.0; 64], &[0.0; 64]);
        only_slot(reader, t0 + at).state
    };

    assert_eq!(
        tick(&mut reader, &mut audio, Duration::ZERO),
        SlotState::Live
    );
    claimed.writer.heartbeat();
    assert_eq!(
        tick(&mut reader, &mut audio, Duration::from_millis(1_500)),
        SlotState::Live
    );
    // 1.9 s after the last heartbeat: still there.
    assert_eq!(
        tick(&mut reader, &mut audio, Duration::from_millis(3_400)),
        SlotState::Live
    );
    assert_eq!(
        tick(&mut reader, &mut audio, Duration::from_millis(3_500)),
        SlotState::Gone
    );

    // A heartbeat brings it back.
    claimed.writer.heartbeat();
    assert_eq!(
        tick(&mut reader, &mut audio, Duration::from_millis(3_600)),
        SlotState::Live
    );
}

#[test]
fn an_exited_host_process_means_gone() {
    let table = TestTable::new();
    let mut reader = table.reader();
    let pid = exited_pid();
    let claimed = table.claim_as(None, pid);
    let info = only_slot(&mut reader, Instant::now());
    assert_eq!(info.host_pid, pid);
    assert_eq!(info.state, SlotState::Gone);
    drop(claimed);
}

#[test]
fn a_still_frame_counter_means_idle() {
    let table = TestTable::new();
    let mut reader = table.reader();
    let claimed = table.claim(None);
    let mut audio = claimed.writer.audio();
    let t0 = Instant::now();

    audio.push(&[0.0; 64], &[0.0; 64]);
    assert_eq!(only_slot(&mut reader, t0).state, SlotState::Live);
    claimed.writer.heartbeat();
    assert_eq!(
        only_slot(&mut reader, t0 + IDLE_AFTER / 2).state,
        SlotState::Live
    );
    assert_eq!(
        only_slot(&mut reader, t0 + IDLE_AFTER).state,
        SlotState::Idle
    );

    audio.push(&[0.0; 64], &[0.0; 64]);
    assert_eq!(
        only_slot(&mut reader, t0 + IDLE_AFTER * 2).state,
        SlotState::Live
    );
}

#[test]
fn a_duplicate_live_id_gets_a_fresh_id() {
    let table = TestTable::new();
    let original = table.claim(Some(7));
    let copy = table.claim(Some(7));
    assert!(!original.id_was_taken);
    assert!(copy.id_was_taken);
    assert_ne!(copy.id, 7);
    assert_ne!(copy.id, 0);
}

#[test]
fn an_id_left_behind_by_a_crashed_host_is_taken_over() {
    let table = TestTable::new();
    let mut reader = table.reader();
    // A DAW crashed without releasing its slot...
    std::mem::forget(table.claim_as(Some(7), exited_pid()));
    // ...and the reopened project brings the same Send Plugin back.
    let reopened = table.claim(Some(7));
    assert_eq!(reopened.id, 7);
    assert!(!reopened.id_was_taken);
    let info = only_slot(&mut reader, Instant::now());
    assert_eq!(info.id, 7);
    assert_eq!(info.state, SlotState::Live);
}

#[test]
fn a_full_table_reuses_slots_of_exited_hosts() {
    let table = TestTable::new();
    let dead = exited_pid();
    let crashed: Vec<Claimed> = (0..SLOT_COUNT)
        .map(|_| table.claim_as(None, dead))
        .collect();
    let _live = table.claim(None);
    drop(crashed);

    // One slot went to `_live`, the other 63 were released.
    let alive: Vec<Claimed> = (1..SLOT_COUNT).map(|_| table.claim(None)).collect();
    let error = Writer::claim_in(&table.0, None, &details("Otter"), std::process::id())
        .err()
        .unwrap();
    assert!(error.to_string().contains("slots are taken"), "{error}");
    drop(alive);
}

#[test]
fn writer_sees_whether_the_app_is_running() {
    let table = TestTable::new();
    let mut claimed = table.claim(None);
    let t0 = Instant::now();
    assert!(!claimed.writer.app_running(t0));

    let reader = table.reader();
    reader.heartbeat();
    assert!(claimed.writer.app_running(t0));
    assert!(
        claimed
            .writer
            .app_running(t0 + Duration::from_millis(1_900))
    );
    assert!(!claimed.writer.app_running(t0 + GONE_AFTER));
    reader.heartbeat();
    assert!(claimed.writer.app_running(t0 + GONE_AFTER));
}

#[test]
fn a_table_with_a_garbage_header_is_rejected() {
    let corruptions: [fn(&Table); 4] = [
        |t| t.header().magic.store(0x0bad_f00d, Relaxed),
        |t| t.header().layout_version.store(99, Relaxed),
        |t| t.header().ring_frames.store(4_096, Relaxed),
        |t| t.header().init.store(0xdead_beef, Relaxed),
    ];
    for corrupt in corruptions {
        let table = TestTable::new();
        // Keep it open: on Windows the table vanishes with its last handle.
        let raw = table.raw();
        corrupt(&raw);
        let error = Reader::open_named(&table.0)
            .err()
            .expect("garbage accepted");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        let error = Writer::claim_in(&table.0, None, &details("Otter"), 1)
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}

#[test]
fn garbage_in_a_slot_is_skipped_without_panicking() {
    let table = TestTable::new();
    let mut reader = table.reader();
    let raw = table.raw();
    let now = Instant::now();

    // Fill every slot with a pseudo-random pattern and mark it live.
    let mut x = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    for (_, slot) in raw.slots() {
        for word in &slot.name {
            word.store(next(), Relaxed);
        }
        slot.name_len.store(next() as u32, Relaxed);
        slot.colour.store(next() as u32, Relaxed);
        slot.flags.store(next() as u32, Relaxed);
        slot.published.store(next(), Relaxed);
        slot.reserved.store(next(), Relaxed);
        slot.generation.store(next() as u32, Relaxed);
        slot.state.store(SLOT_LIVE, Relaxed);
    }
    assert!(reader.slots(now).is_empty());

    // A slot that passes the details check but has nonsense counters still reads safely.
    let slot = raw.slot(3);
    slot.write_details(&details("Otter"));
    slot.host_pid.store(std::process::id(), Relaxed);
    let info = only_slot(&mut reader, now);
    let mut out = vec![0.0; 256];
    for since in [0, 1, u64::MAX] {
        let read = reader.read(info.slot, since, &mut out).unwrap();
        assert!(read.frames <= 128);
    }

    // An unknown state value means the slot isn't listed.
    slot.state.store(77, Relaxed);
    assert!(reader.slots(now).is_empty());
}
