//! The hand-off of snapshots from the UI thread to the audio thread (ADR 18),
//! as `tla/SnapshotHandoff.tla` specifies it with a queue of one.
//!
//! The UI thread publishes a snapshot into a slot. At the start of a block the
//! audio thread takes it, but only when the retired place is empty, and puts
//! the snapshot it played there. The UI thread frees what it finds in either.
//! So the audio thread never waits, allocates or frees.

use std::ptr;

#[cfg(loom)]
use loom::sync::{
    Arc,
    atomic::{AtomicPtr, Ordering},
};
#[cfg(not(loom))]
use std::sync::{
    Arc,
    atomic::{AtomicPtr, Ordering},
};

struct Shared<T> {
    /// The newest snapshot, not yet taken by the audio thread, or null.
    slot: AtomicPtr<T>,
    /// The snapshot the audio thread played before its current one, not yet
    /// freed by the UI thread, or null. Only the audio thread sets it and only
    /// the UI thread clears it.
    retired: AtomicPtr<T>,
}

impl<T> Drop for Shared<T> {
    fn drop(&mut self) {
        for place in [&self.slot, &self.retired] {
            let snapshot = place.load(Ordering::Acquire);
            if !snapshot.is_null() {
                // SAFETY: both threads are gone, and every non-null pointer in
                // a place came from `Box::into_raw` and is owned by the place.
                drop(unsafe { Box::from_raw(snapshot) });
            }
        }
    }
}

/// The UI thread's end.
pub struct Publisher<T> {
    shared: Arc<Shared<T>>,
}

/// The audio thread's end. Dropping it frees the snapshot it plays, so drop it
/// only once the audio stream has stopped.
pub struct Player<T> {
    shared: Arc<Shared<T>>,
    playing: Option<Box<T>>,
}

pub fn handoff<T>() -> (Publisher<T>, Player<T>) {
    let shared = Arc::new(Shared {
        slot: AtomicPtr::new(ptr::null_mut()),
        retired: AtomicPtr::new(ptr::null_mut()),
    });
    (
        Publisher {
            shared: shared.clone(),
        },
        Player {
            shared,
            playing: None,
        },
    )
}

impl<T> Publisher<T> {
    /// Makes `snapshot` the next one the audio thread plays. Call
    /// [`collect`](Self::collect) as well, or the audio thread cannot take it.
    pub fn publish(&self, snapshot: T) {
        let new = Box::into_raw(Box::new(snapshot));
        let old = self.shared.slot.swap(new, Ordering::AcqRel);
        if !old.is_null() {
            // SAFETY: the audio thread never saw `old`, since it was still in
            // the slot, and the slot owned it.
            drop(unsafe { Box::from_raw(old) });
        }
    }

    /// Frees the snapshot the audio thread retired, if any, which makes room
    /// for it to take the next one.
    pub fn collect(&self) {
        let old = self.shared.retired.swap(ptr::null_mut(), Ordering::Acquire);
        if !old.is_null() {
            // SAFETY: the audio thread let go of `old` when it retired it.
            drop(unsafe { Box::from_raw(old) });
        }
    }
}

impl<T> Player<T> {
    /// The snapshot to play this block: the newest published one if the
    /// retired place is free, otherwise the one played before.
    pub fn current(&mut self) -> Option<&T> {
        if self.shared.retired.load(Ordering::Acquire).is_null() {
            let new = self.shared.slot.swap(ptr::null_mut(), Ordering::AcqRel);
            if !new.is_null() {
                // SAFETY: the slot owned `new`, and swapping it out moved that
                // ownership here.
                let new = unsafe { Box::from_raw(new) };
                if let Some(old) = self.playing.replace(new) {
                    // Only this thread sets the retired place, and it was
                    // empty, so this overwrites nothing. A store would do, but
                    // loom then lets a later swap in `collect` miss it
                    // (https://github.com/tokio-rs/loom/issues/254).
                    let overwritten = self
                        .shared
                        .retired
                        .swap(Box::into_raw(old), Ordering::AcqRel);
                    debug_assert!(overwritten.is_null());
                }
            }
        }
        self.playing.as_deref()
    }
}
