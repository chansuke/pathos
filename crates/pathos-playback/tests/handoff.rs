//! The hand-off checked against the invariants of `tla/SnapshotHandoff.tla`.
//! The loom tests run every interleaving of the two threads within their
//! bound (ADR 17, ADR 18):
//! `RUSTFLAGS="--cfg loom" cargo test -p pathos-playback --release`.

#![expect(
    clippy::unwrap_used,
    reason = "helpers outside #[test] functions fail the test by panicking"
)]

#[cfg(loom)]
use loom::{sync::Arc, thread};
#[cfg(not(loom))]
use std::thread;

use std::cell::Cell;
use std::sync::{Arc as StdArc, Mutex as StdMutex};

#[cfg(loom)]
use pathos_playback::engine::Transport;
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
/// audio thread. Uses `std::sync::Mutex` so reading `freed` on the audio
/// thread checks `NoUseAfterFree` without adding synchronization edges to
/// loom's memory model.
struct Snapshot {
    id: usize,
    freed: StdArc<StdMutex<Vec<usize>>>,
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

/// Runs the UI `publish_phase` while the audio thread plays `blocks` blocks,
/// checking directly on the audio thread that each snapshot is not freed while
/// read (`NoUseAfterFree`), never freed on the audio thread, played in order,
/// settles on `snapshots`, and that each snapshot is freed exactly once.
fn run_scenario(
    snapshots: usize,
    blocks: usize,
    publish_phase: impl FnOnce(
        &pathos_playback::handoff::Publisher<Snapshot>,
        &StdArc<StdMutex<Vec<usize>>>,
    ),
) {
    let freed = StdArc::new(StdMutex::new(Vec::new()));
    let audio_freed = freed.clone();
    let (publisher, mut player) = handoff::<Snapshot>();
    let audio = thread::spawn(move || {
        ON_AUDIO_THREAD.with(|on| on.set(true));
        let mut played = Vec::new();
        for _ in 0..blocks {
            if let Some(snapshot) = player.current() {
                let id = snapshot.id;
                assert!(
                    !audio_freed.lock().unwrap().contains(&id),
                    "snapshot {id} was used after being freed"
                );
                played.push(id);
            }
        }
        ON_AUDIO_THREAD.with(|on| on.set(false));
        (player, played)
    });
    publish_phase(&publisher, &freed);
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

/// The UI thread publishes snapshots 1 to `snapshots`, collecting after each,
/// while the audio thread plays `blocks` blocks.
fn publish_while_playing(snapshots: usize, blocks: usize) {
    run_scenario(snapshots, blocks, |publisher, freed| {
        for id in 1..=snapshots {
            publisher.publish(Snapshot {
                id,
                freed: freed.clone(),
            });
            publisher.collect();
        }
    });
}

/// The UI thread publishes snapshots 1 to `snapshots` in a row without
/// calling `collect` in between, then calls `collect`, exercising slot
/// overwrites and a full retired place concurrently with playback.
fn publish_burst_then_collect(snapshots: usize, blocks: usize) {
    run_scenario(snapshots, blocks, |publisher, freed| {
        for id in 1..=snapshots {
            publisher.publish(Snapshot {
                id,
                freed: freed.clone(),
            });
        }
        publisher.collect();
    });
}

#[cfg(loom)]
#[test]
fn every_interleaving_keeps_the_invariants() {
    loom::model(|| publish_while_playing(3, 4));
}

#[cfg(loom)]
#[test]
fn consecutive_publishes_without_collect_keep_the_invariants() {
    loom::model(|| publish_burst_then_collect(4, 3));
}

/// A `seek` written before `play` is always observed whenever the audio thread
/// observes `playing == true`.
#[cfg(loom)]
#[test]
fn transport_seek_before_play_is_observed_when_playing() {
    loom::model(|| {
        let transport = Arc::new(Transport::default());
        let audio_transport = transport.clone();
        let audio = thread::spawn(move || {
            let mut position = 100u64;
            for _ in 0..2 {
                let (playing, seek) = audio_transport.begin_block();
                if let Some(frame) = seek {
                    position = frame;
                }
                if playing {
                    assert_eq!(position, 0, "observed playing without the prior seek");
                }
            }
        });
        transport.seek(0);
        transport.play();
        audio.join().unwrap();
    });
}

/// Concurrent `meter` on the audio thread and `take_peaks` on the UI thread
/// never lose a peak on either channel (ADR 25).
#[cfg(loom)]
#[test]
fn transport_meter_and_take_peaks_never_lose_a_peak() {
    loom::model(|| {
        let transport = Arc::new(Transport::default());
        let audio_transport = transport.clone();
        let audio = thread::spawn(move || {
            audio_transport.meter(&[[0.25, -0.5]]);
            audio_transport.meter(&[[-0.75, 0.25]]);
        });
        let mut seen = [0.0f32; 2];
        for _ in 0..2 {
            let take = transport.take_peaks();
            seen[0] = seen[0].max(take[0]);
            seen[1] = seen[1].max(take[1]);
        }
        audio.join().unwrap();
        let final_take = transport.take_peaks();
        seen[0] = seen[0].max(final_take[0]);
        seen[1] = seen[1].max(final_take[1]);
        assert_eq!(seen, [0.75, 0.5]);
        assert_eq!(transport.take_peaks(), [0.0, 0.0]);
    });
}

#[cfg(not(loom))]
#[test]
fn concurrent_runs_keep_the_invariants() {
    for _ in 0..200 {
        publish_while_playing(50, 50);
    }
}

#[cfg(not(loom))]
#[test]
fn concurrent_bursts_keep_the_invariants() {
    for _ in 0..200 {
        publish_burst_then_collect(50, 50);
    }
}

#[cfg(not(loom))]
mod sequential {
    use super::*;

    fn snapshot(id: usize, freed: &StdArc<StdMutex<Vec<usize>>>) -> Snapshot {
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
        let freed = StdArc::new(StdMutex::new(Vec::new()));
        let (publisher, mut player) = handoff();
        publisher.publish(snapshot(1, &freed));
        publisher.publish(snapshot(2, &freed));
        assert_eq!(*freed.lock().unwrap(), [1]);
        assert_eq!(player.current().map(|snapshot| snapshot.id), Some(2));
    }

    #[test]
    fn the_player_keeps_its_snapshot_until_the_retired_one_is_collected() {
        let freed = StdArc::new(StdMutex::new(Vec::new()));
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
        let freed = StdArc::new(StdMutex::new(Vec::new()));
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
