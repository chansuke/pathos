---
number: 18
title: Play back in real time from snapshots handed to the audio thread
status: accepted
date: 2026-11-16
---

# Play back in real time from snapshots handed to the audio thread

## Context and Problem Statement

pathos renders a project offline (ADR 15) and edits it in a window, but cannot
play it. Playback runs the synthesis on an audio thread that the operating
system calls every few milliseconds, while the user keeps editing on the UI
thread. How does the audio thread get the project, and how is what it plays
held to the offline render?

## Decision Drivers

* The audio callback must never wait for a lock, allocate or free memory, or
  it may miss its deadline and click.
* What a user hears must be the offline render (ADR 15), sample for sample.
* An edit made during playback must be heard within a block or two.
* The project changes only through commands (ADR 2), and the audio thread is
  not a place to apply them.
* The hand-off between threads is specified and checked (ADR 17).

## Considered Options

* Immutable snapshots handed over through a slot, retired through a queue
* A project behind a mutex shared by both threads
* Commands sent to the audio thread, which keeps its own copy of the project

## Decision Outcome

Chosen option: "Immutable snapshots handed over through a slot, retired through
a queue", because the audio thread then never waits, allocates or frees, and the
protocol is small enough to check every interleaving of it.

After each accepted edit, the UI thread builds a snapshot: the note schedule
(ADR 15) at the device's sample rate, with what the instrument needs, in an
immutable allocation. It publishes the snapshot by swapping it into a
one-place slot. A snapshot it swaps out was never seen by the audio thread, so
the UI thread frees it.

At the start of each block, the audio thread checks that a second one-place
slot back to the UI thread, a queue of one, is empty, swaps the first slot
empty, and puts the snapshot it was playing into the second. Without room it
keeps playing the snapshot it has. The UI thread empties the second slot and
frees what it finds. Only the audio thread fills it and only the UI thread
empties it, so the room the audio thread saw cannot vanish before it fills it. `tla/SnapshotHandoff.tla` specifies this protocol, and TLC checks that
no snapshot is used after it is freed, that each is freed exactly once, and
that the audio thread settles on the last one published.

The transport (play, stop and seek) is sent to the audio thread through a
second bounded queue, and the audio thread reports the frame it plays through
an atomic. The transport is not part of the project and is not a command.

`pathos-render` gains a player that renders block by block and keeps each
sounding note's phase between blocks. Its blocks, joined, are the offline
render bit for bit, and a property test checks this for generated projects and
block sizes. The schedule is computed at the device's rate, so no resampling
happens on the way to the device.

Audio output uses `cpal`, which covers ALSA, CoreAudio and WASAPI. Both slots
are atomic pointers rather than an `rtrb` ring buffer, so that loom can run the
hand-off; the specification holds for any length of queue.

### Consequences

* Good, because the audio thread never waits, allocates or frees, as the
  specification shows for every interleaving within its bound.
* Good, because what is heard equals the offline render, so every test of the
  render also covers playback.
* Good, because edits stay commands on the UI thread, and the audio thread only
  sees their results.
* Bad, because each edit builds a whole new snapshot, which costs time on the
  UI thread for large projects. Sharing unchanged parts between snapshots can
  come later.
* Bad, because the slots hold unsafe atomic pointers, which only the
  specification and loom tests check.
* Bad, because the Linux build needs ALSA in the dev shell and on CI.
* Neutral, because an edit lands at the next block boundary, a few
  milliseconds late.

### Confirmation

* CI checks `tla/SnapshotHandoff.tla` with TLC.
* loom tests run the Rust hand-off under every interleaving within their bound
  and assert the invariants of the specification: no snapshot is dropped on the
  audio thread, used after it is freed, or freed twice.
* A property test checks that the player's blocks, joined, equal the offline
  render.

## Pros and Cons of the Options

### Immutable snapshots handed over through a slot, retired through a queue

* Good, because the audio thread does a bounded amount of work per block.
* Bad, because the protocol needs care, which is why it is specified.

### A project behind a mutex shared by both threads

* Good, because it is the simplest to write.
* Bad, because the audio thread waits whenever the UI thread holds the lock,
  which causes clicks.

### Commands sent to the audio thread, which keeps its own copy of the project

* Good, because only small commands cross between threads.
* Bad, because applying a command can allocate, and the audio thread would
  rebuild the schedule in its callback.

## More Information

Loop playback, a metronome and input monitoring are left to later decisions.
The tla-ts comparison that ADR 17 asks for is recorded there.
