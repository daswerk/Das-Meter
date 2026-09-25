//! The Send Plugin in an in-process host, against a real transport table.

mod common;

use std::sync::Mutex;
use std::time::Instant;

use common::Instance;
use dasmeter_send::identity::is_animal;
use dasmeter_transport::{Reader, SlotInfo, remove_table};

/// `use_table` is process-wide, so these tests take turns.
static SERIAL: Mutex<()> = Mutex::new(());

fn table(test: &str) -> String {
    format!("dmsp-{test}-{}", std::process::id())
}

fn slots(reader: &mut Reader) -> Vec<SlotInfo> {
    reader.slots(Instant::now())
}

fn ramp(n: usize, scale: f32) -> Vec<f32> {
    (0..n).map(|i| scale * i as f32 / n as f32).collect()
}

#[test]
fn stereo_passes_through_and_reaches_the_app_while_listened_to() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let name = table("pass");
    let mut plugin = Instance::new(&name, None, 512);
    let mut reader = Reader::open_named(&name).unwrap();

    let listed = slots(&mut reader);
    assert_eq!(listed.len(), 1);
    assert!(
        is_animal(&listed[0].details.name),
        "{:?}",
        listed[0].details
    );
    assert_eq!(listed[0].details.sample_rate, 48_000);
    assert!(!listed[0].details.mono);

    let (mut left, mut right) = (ramp(256, 0.5), ramp(256, -0.25));
    let mut out = [vec![0.0; 256], vec![0.0; 256]];

    // Not listened to: passes through, nothing to read.
    plugin.process(&mut left, &mut right, &mut out);
    assert_eq!(out[0], left);
    assert_eq!(out[1], right);
    let slot = listed[0].slot;
    let cursor = reader.cursor(slot).unwrap();
    let mut buffer = vec![0.0; 1024];
    assert_eq!(reader.read(slot, cursor, &mut buffer).unwrap().frames, 0);

    // Listened to: the block arrives interleaved.
    reader.set_listened(slot, true);
    plugin.process(&mut left, &mut right, &mut out);
    let read = reader.read(slot, cursor, &mut buffer).unwrap();
    assert_eq!(read.frames, 256);
    for i in 0..256 {
        assert_eq!(buffer[2 * i], left[i]);
        assert_eq!(buffer[2 * i + 1], right[i]);
    }
    assert_eq!(out[0], left);

    drop(plugin);
    assert!(slots(&mut reader).is_empty(), "the slot is released");
    remove_table(&name);
}

#[test]
fn identity_survives_a_project_reload() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let name = table("reload");
    let mut reader = Reader::open_named(&name).unwrap();

    let mut plugin = Instance::new(&name, None, 512);
    let before = slots(&mut reader).remove(0);
    let state = plugin.save();
    drop(plugin);

    let _plugin = Instance::new(&name, Some(&state), 512);
    let after = slots(&mut reader).remove(0);
    assert_eq!(after.id, before.id);
    assert_eq!(after.details, before.details, "same name and colour");

    remove_table(&name);
}

#[test]
fn a_duplicated_track_gets_a_fresh_id_and_animal() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let name = table("dup");
    let mut reader = Reader::open_named(&name).unwrap();

    let mut original = Instance::new(&name, None, 512);
    let state = original.save();
    // The DAW duplicates the track: a second instance loads the same state.
    let mut copy = Instance::new(&name, Some(&state), 512);

    let listed = slots(&mut reader);
    assert_eq!(listed.len(), 2);
    assert_ne!(listed[0].id, listed[1].id);
    assert_ne!(listed[0].details.name, listed[1].details.name);
    assert!(listed.iter().all(|s| is_animal(&s.details.name)));

    // The copy saves its new identity, so the next reload keeps them apart.
    assert_ne!(copy.save(), state);
    assert_eq!(original.save(), state);

    drop((original, copy));
    remove_table(&name);
}

#[test]
fn without_a_host_timer_the_heartbeat_keeps_going() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let name = table("beat");
    let mut reader = Reader::open_named(&name).unwrap();
    // The test host offers no timer, as clap-wrapper's AU doesn't.
    let plugin = Instance::new(&name, None, 512);

    let t0 = Instant::now();
    reader.slots(t0);
    std::thread::sleep(std::time::Duration::from_millis(600));
    // Judged 3 s on: gone, unless the heartbeat moved in between.
    let later = reader.slots(t0 + std::time::Duration::from_secs(3));
    assert_ne!(later[0].state, dasmeter_transport::SlotState::Gone);

    drop(plugin);
    remove_table(&name);
}
