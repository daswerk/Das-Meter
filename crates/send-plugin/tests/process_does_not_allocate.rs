//! The Send Plugin's `process()` must not allocate (spec: no allocs, locks or
//! syscalls on the audio thread). A counting allocator catches any allocation
//! made on this thread while the in-process host calls `process()`.

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::time::Instant;

use common::Instance;
use dasmeter_transport::{Reader, remove_table};

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
fn process_makes_no_allocations() {
    let name = format!("dmspa{}", std::process::id());
    let mut plugin = Instance::new(&name, None, 4_096);
    let mut reader = Reader::open_named(&name).unwrap();
    let slot = reader.slots(Instant::now())[0].slot;

    let mut left = vec![0.25_f32; 4_096];
    let mut right = vec![-0.25_f32; 4_096];
    let mut run = |plugin: &mut Instance, block: usize, times: usize| {
        let mut out = [vec![0.0_f32; block], vec![0.0_f32; block]];
        allocations_during(|| {
            for _ in 0..times {
                plugin.process(&mut left[..block], &mut right[..block], &mut out);
            }
        })
    };

    let not_listened = run(&mut plugin, 64, 1_000);
    reader.set_listened(slot, true);
    let listened: Vec<usize> = [1, 64, 512, 4_096]
        .into_iter()
        .map(|block| run(&mut plugin, block, 100))
        .collect();

    assert_eq!(not_listened, 0);
    assert_eq!(listened, [0, 0, 0, 0]);
    // The counter itself works.
    assert!(allocations_during(|| drop(vec![1_u8; 16])) > 0);

    drop(plugin);
    remove_table(&name);
}
