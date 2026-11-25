//! The hand-off checked against the invariants of `tla/SnapshotHandoff.tla`.
//! The loom tests run every interleaving of the two threads within their
//! bound (ADR 17, ADR 18):
//! `RUSTFLAGS="--cfg loom" cargo test -p pathos-playback --release`.

#![expect(
    clippy::unwrap_used,
    reason = "helpers outside #[test] functions fail the test by panicking"
)]

#[cfg(loom)]
use loom::{
    sync::{Arc, Mutex},
    thread,
};
#[cfg(not(loom))]
use std::{
    sync::{Arc, Mutex},
    thread,
};

use std::cell::Cell;

use pathos_playback::handoff::handoff;

#[cfg(loom)]
loom::thread_local! {
    static ON_AUDIO_THREAD: Cell<bool> = Cell::new(false);
}
#[cfg(not(loom))]
thread_local! {
    static ON_AUDIO_THREAD: Cell<bool> = const { Cell::new(false) };
}

/// A snapshot that records when it is freed, and fails if that happens on the
/// audio thread.
struct Snapshot {
    id: usize,
    freed: Arc<Mutex<Vec<usize>>>,
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        assert!(
            !ON_AUDIO_THREAD.with(Cell::get),
            "snapshot {} is freed on the audio thread",
            self.id
        );
        self.freed.lock().unwrap().push(self.id);
    }
}

/// The UI thread publishes snapshots 1 to `snapshots` while the audio thread
/// plays `blocks` blocks. Then each runs once more, as they would while
/// playback goes on. Checks that the audio thread plays snapshots in order,
/// never frees one, settles on the last, and that each other snapshot is
/// freed exactly once and the last once both ends are dropped.
fn publish_while_playing(snapshots: usize, blocks: usize) {
    let freed = Arc::new(Mutex::new(Vec::new()));
    let (publisher, mut player) = handoff::<Snapshot>();
    let audio = thread::spawn(move || {
        ON_AUDIO_THREAD.with(|on| on.set(true));
        let played: Vec<usize> = (0..blocks)
            .filter_map(|_| player.current().map(|snapshot| snapshot.id))
            .collect();
        ON_AUDIO_THREAD.with(|on| on.set(false));
        (player, played)
    });
    for id in 1..=snapshots {
        publisher.publish(Snapshot {
            id,
            freed: freed.clone(),
        });
        publisher.collect();
    }
    let (mut player, played) = audio.join().unwrap();
    assert!(played.is_sorted(), "played out of order: {played:?}");

    publisher.collect();
    player.current();
    publisher.collect();
    assert_eq!(
        player.current().map(|snapshot| snapshot.id),
        Some(snapshots)
    );
    let mut before_last = freed.lock().unwrap().clone();
    before_last.sort_unstable();
    assert_eq!(before_last, (1..snapshots).collect::<Vec<_>>());

    drop(player);
    drop(publisher);
    let mut all = freed.lock().unwrap().clone();
    all.sort_unstable();
    assert_eq!(all, (1..=snapshots).collect::<Vec<_>>());
}

#[cfg(loom)]
#[test]
fn every_interleaving_keeps_the_invariants() {
    loom::model(|| publish_while_playing(3, 4));
}

#[cfg(not(loom))]
#[test]
fn concurrent_runs_keep_the_invariants() {
    for _ in 0..200 {
        publish_while_playing(50, 50);
    }
}

#[cfg(not(loom))]
mod sequential {
    use super::*;

    fn snapshot(id: usize, freed: &Arc<Mutex<Vec<usize>>>) -> Snapshot {
        Snapshot {
            id,
            freed: freed.clone(),
        }
    }

    #[test]
    fn nothing_plays_before_a_snapshot_is_published() {
        let (_publisher, mut player) = handoff::<Snapshot>();
        assert!(player.current().is_none());
    }

    #[test]
    fn a_snapshot_replaced_before_it_is_played_is_freed_at_once() {
        let freed = Arc::new(Mutex::new(Vec::new()));
        let (publisher, mut player) = handoff();
        publisher.publish(snapshot(1, &freed));
        publisher.publish(snapshot(2, &freed));
        assert_eq!(*freed.lock().unwrap(), [1]);
        assert_eq!(player.current().map(|snapshot| snapshot.id), Some(2));
    }

    #[test]
    fn the_player_keeps_its_snapshot_until_the_retired_one_is_collected() {
        let freed = Arc::new(Mutex::new(Vec::new()));
        let (publisher, mut player) = handoff();
        publisher.publish(snapshot(1, &freed));
        assert_eq!(player.current().map(|snapshot| snapshot.id), Some(1));
        publisher.publish(snapshot(2, &freed));
        assert_eq!(player.current().map(|snapshot| snapshot.id), Some(2));
        // 1 is retired and not yet collected, so 3 waits in the slot.
        publisher.publish(snapshot(3, &freed));
        assert_eq!(player.current().map(|snapshot| snapshot.id), Some(2));
        assert!(freed.lock().unwrap().is_empty());
        publisher.collect();
        assert_eq!(*freed.lock().unwrap(), [1]);
        assert_eq!(player.current().map(|snapshot| snapshot.id), Some(3));
        publisher.collect();
        assert_eq!(*freed.lock().unwrap(), [1, 2]);
    }

    #[test]
    fn dropping_both_ends_frees_what_they_hold() {
        let freed = Arc::new(Mutex::new(Vec::new()));
        let (publisher, mut player) = handoff();
        publisher.publish(snapshot(1, &freed));
        player.current();
        publisher.publish(snapshot(2, &freed));
        player.current();
        publisher.publish(snapshot(3, &freed));
        drop(player);
        drop(publisher);
        let mut all = freed.lock().unwrap().clone();
        all.sort_unstable();
        assert_eq!(all, [1, 2, 3]);
    }
}
