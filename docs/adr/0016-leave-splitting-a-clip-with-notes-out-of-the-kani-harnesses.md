---
number: 16
title: Leave splitting a clip with notes out of the Kani harnesses
status: accepted
date: 2026-11-16
---

# Leave splitting a clip with notes out of the Kani harnesses

## Context and Problem Statement

ADR 14 gives clips notes, and ADR 11 checks each command with Kani on projects
of fixed size. The first attempt made the harnesses far more expensive. With a
`Vec<Note>` inside `Clip`, `move_clip_of_2`, which takes no notes, went from
20 s and 0.8 GB to more than 12 GB and was killed, locally and in CI. How
should the harnesses cover notes and stay within the solver's time and
memory?

## Decision Drivers

* Each harness must finish within the ten-minute limit of ADR 11, and the
  whole workflow within the time GitHub gives a job.
* Harnesses for commands that do not touch notes must not get slower.
* Every rule of a command must still be checked by some test, with the reason
  written down where Kani does not reach it.

## Considered Options

* Keep notes in one list in the Rust core, check notes only where they can
  change, and leave a split with notes to the other tests
* Keep looking for a cheaper harness for a split with notes
* Drop the Kani harnesses of every command that touches notes

## Decision Outcome

Chosen option: "Keep notes in one list in the Rust core, check notes only where
they can change, and leave a split with notes to the other tests", because it
keeps every other harness as cheap as before and every rule tested.

The decision has five parts, each measured locally with Kani 0.68.0 and
MiniSat, with three or four harnesses running side by side:

* The Rust core keeps the notes of all clips in one list ordered by clip id,
  start and pitch, and `Clip` stays a plain value. The model keeps notes inside
  their clip (ADR 14); the differential tests check that the two behave the
  same.
* The harnesses check the notes of the result only when the project starts
  with notes or the command is `AddNote`. Without notes to start from, the check
  only shows that other commands do not create one, and it was most of the
  cost: `move_clip_of_2` took 361 s with the check and 36 s without it.
* There is no harness for `SplitClip` on a clip with notes. With one note it
  ran past ten minutes, after it had used more than 10 GB in an earlier
  version. A split with notes is checked by the differential tests, the
  mutants of ADR 13, hand-written expected values, and the model's proved
  theorem `splitClip_keeps_onsets`.
* The Rust core finds the notes of a clip by a linear search. An insertion
  moves the notes after it anyway, and with a binary search
  `remove_note_of_1` took 451 s locally and ran past ten minutes in CI; with a
  linear search it took 149 s.
* The Kani workflow runs each harness in its own job, listed by
  `cargo kani list`, so that the note harnesses, which take minutes each,
  run side by side and each has a runner's memory to itself. A job named
  `verify` passes when every harness has passed.

This amends ADR 11: Kani checks a split only on clips without notes.

### Consequences

* Good, because the harnesses that do not touch notes take as long as before,
  from 4 s to 52 s.
* Good, because a new harness gets its own job without a change to the
  workflow.
* Bad, because overflow and panics in the note handling of a split are not
  checked for every input, only on generated and hand-written ones.
* Bad, because the Rust core and the model store notes differently, so the
  correspondence rests on the differential tests.
* Bad, because CI took about twice as long as this machine, so the slowest
  harness, `add_note_to_2_notes` at 277 s locally, is close to the limit, and a
  larger harness needs a new measurement.

### Confirmation

The Kani workflow passes with every harness listed by `cargo kani list`, and
`harnesses` in `verification.rs` names the harnesses of each command, with the
reason that a split with notes has none.

## Pros and Cons of the Options

### Keep notes in one list in the Rust core, check notes only where they can change, and leave a split with notes to the other tests

* Good, because each step was measured, and the cost went where it was found.
* Bad, because one rule has no bounded proof.

### Keep looking for a cheaper harness for a split with notes

* Good, because every command would keep a harness.
* Bad, because neither moving the notes in place with `rotate_left` nor a
  smaller harness brought it under the limit, and no further approach was
  known to work.

### Drop the Kani harnesses of every command that touches notes

* Good, because the workflow would stay as fast as before.
* Bad, because adding and removing a note would lose the checks that do fit.
