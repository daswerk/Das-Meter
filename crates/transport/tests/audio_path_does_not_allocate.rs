//! The Send Plugin's audio path must not allocate (spec: no allocs, locks or
//! syscalls on the audio thread). A counting allocator catches any allocation
//! made on this thread while pushing.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use dasmeter_transport::{Details, Reader, Writer, remove_table};

struct CountingAllocator;

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

fn note_allocation() {
    let _ = COUNTING.try_with(|counting| {
        if counting.get() {
            let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
        }
    });
}

// SAFETY: forwards to the system allocator unchanged.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        note_allocation();
        // SAFETY: same contract as ours.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        note_allocation();
        // SAFETY: same contract as ours.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        note_allocation();
        // SAFETY: same contract as ours.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn allocations_during(f: impl FnOnce()) -> usize {
    ALLOCATIONS.with(|n| n.set(0));
    COUNTING.with(|counting| counting.set(true));
    f();
    COUNTING.with(|counting| counting.set(false));
    ALLOCATIONS.with(|n| n.get())
}

#[test]
fn pushing_audio_makes_no_allocations() {
    let name = format!("dmta{}", std::process::id());
    let details = Details {
        name: "Otter".into(),
        colour: 0xff8800,
        mono: false,
        sample_rate: 48_000,
    };
    let claimed = Writer::claim_named(&name, None, &details).unwrap();
    let mut reader = Reader::open_named(&name).unwrap();
    let slot = reader.slots(std::time::Instant::now())[0].slot;
    let mut audio = claimed.writer.audio();
    let left = vec![0.25_f32; 4_096];
    let right = vec![-0.25_f32; 4_096];

    // Not listened to, listened to, and blocks larger than the ring.
    let not_listened = allocations_during(|| {
        for _ in 0..1_000 {
            audio.push(&left[..64], &right[..64]);
        }
    });
    reader.set_listened(slot, true);
    let listened = allocations_during(|| {
        for block in [1, 64, 512, 4_096] {
            for _ in 0..100 {
                audio.push(&left[..block], &right[..block]);
            }
        }
    });
    let huge = vec![0.0_f32; 40_000];
    let oversized = allocations_during(|| audio.push(&huge, &huge));

    assert_eq!(not_listened, 0);
    assert_eq!(listened, 0);
    assert_eq!(oversized, 0);

    // The counter itself works.
    assert!(allocations_during(|| drop(vec![1_u8; 16])) > 0);

    drop(claimed);
    remove_table(&name);
}
