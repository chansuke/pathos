//! The audio thread must not allocate or free (ADR 18). This binary counts
//! every allocation and free made while the engine fills a block.

#![cfg(not(loom))]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::num::NonZeroU32;
use std::sync::Arc;

use pathos_core::{Command, Edit, History, PPQ};
use pathos_playback::engine::{Engine, Transport};
use pathos_playback::handoff::handoff;
use pathos_render::Score;

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static COUNT: Cell<usize> = const { Cell::new(0) };
}

struct Counting;

fn count() {
    // `try_with` because the allocator also runs while thread locals are
    // being torn down.
    let _ = COUNTING.try_with(|counting| {
        if counting.get() {
            COUNT.with(|count| count.set(count.get() + 1));
        }
    });
}

// SAFETY: it forwards every call to the system allocator.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        count();
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count();
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Runs `f` and returns how many times it allocated or freed.
fn allocations(f: impl FnOnce()) -> usize {
    COUNT.with(|count| count.set(0));
    COUNTING.with(|counting| counting.set(true));
    f();
    COUNTING.with(|counting| counting.set(false));
    COUNT.with(Cell::get)
}

#[test]
fn filling_blocks_neither_allocates_nor_frees() {
    let mut history = History::default();
    history.edit(Edit::Apply(Command::AddTrack)).unwrap();
    history
        .edit(Edit::Apply(Command::AddClip {
            track: 0,
            start: 0,
            length: 4 * PPQ,
        }))
        .unwrap();
    for pitch in [60, 64, 67] {
        history
            .edit(Edit::Apply(Command::AddNote {
                id: 0,
                start: 0,
                length: 4 * PPQ,
                pitch,
                velocity: 90,
            }))
            .unwrap();
    }
    let rate = NonZeroU32::new(48_000).unwrap();
    let (publisher, player) = handoff();
    let transport = Arc::new(Transport::default());
    let mut engine = Engine::new(player, transport.clone());
    let mut out = vec![0.0; 10_000 * 2];
    transport.play();

    for edit in 0..4 {
        // The last score loops a beat, which the blocks below cross, and
        // clicks on the beats.
        let score = if edit < 3 {
            Score::new(history.present(), rate)
        } else {
            Score::new(history.present(), rate)
                .looped(0..PPQ)
                .with_metronome()
        };
        publisher.publish(score);
        publisher.collect();
        transport.seek(edit * 1000);
        // Taking a new score, rendering in parts and retiring the old score.
        let made = allocations(|| {
            engine.fill(&mut out, 2);
            engine.fill(&mut out[..256], 2);
        });
        assert_eq!(made, 0, "edit {edit}");
    }
}
