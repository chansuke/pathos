---
number: 17
title: Specify the concurrent parts in TLA+ and check them with TLC
status: proposed
date: 2026-11-16
---

# Specify the concurrent parts in TLA+ and check them with TLC

## Context and Problem Statement

The Lean model specifies the project core, a pure function from a project and a
command to the next project (ADR 2, ADR 3). It leaves out what runs
concurrently: ADR 2 says the state of the audio thread needs its own
specification, and ADR 3 leaves the hand-off between the UI and audio threads
to other oracles. Real-time playback (ADR 15) brings these parts in:

* the audio thread switches to a new snapshot of the project while the UI
  edits, and an old snapshot is freed only after the audio thread has let go
  of it;
* the transport plays, stops, loops and seeks while edits arrive;
* an agent's proposal (ADR 5, ADR 6) arrives while the user keeps editing,
  which ADR 10 left to the agent API.

Bugs in these parts depend on the order in which threads run, so tests that
run each order once rarely find them. How are they specified and checked?

## Decision Drivers

* Every interleaving within a bound must be checked, not a sample of them.
* The pure core keeps one specification, the Lean model; a second one must not
  restate `step`.
* The checker must run in CI from the Nix dev shell, as the model does.
* The implementation must be held to the specification, as the differential
  tests hold the Rust core to the model (ADR 4).

## Considered Options

* TLA+ checked by TLC, with tla-ts as a second checker to compare
* TLA+ checked by tla-ts alone
* Quint checked by Apalache
* Interleavings modelled in Lean
* Testing the Rust code with loom only

## Decision Outcome

Chosen option: "TLA+ checked by TLC, with tla-ts as a second checker to
compare", because TLA+ is made for interleavings, TLC is the reference checker
and is in nixpkgs, and tla-ts can be adopted later by measurement rather than
by its claims.

TLA+ specifies only the protocols between concurrent actors. A command is an
opaque value in a TLA+ specification: what it does to a project is the Lean
model's, and TLA+ only says when it is applied and by whom. The specifications
live in their own directory, and CI checks each with TLC on bounded constants.

The first specification is the snapshot hand-off of playback. It is checked
with both TLC and tla-ts, and the two are compared on the states they report
and on their time. tla-ts takes TLC's place only if they agree on every
specification for some time, in the same way ADR 11 chose a solver by
measurement.

This ADR stays proposed until playback is designed and the open questions
below are settled.

### Consequences

* Good, because every interleaving up to the bound is checked, which tests of
  the Rust code cannot do.
* Good, because the Lean model stays the only specification of what a command
  does.
* Bad, because contributors need to read TLA+ as well as Lean.
* Bad, because a TLA+ specification says nothing about the Rust code until a
  conformance check exists.
* Bad, because TLC checks only small constants, so a bug that needs many
  threads or long histories can be missed.

### Confirmation

The TLA+ workflow checks each specification in `tla/` with TLC on the constants
in its `.cfg` file, and fails unless TLC reports that no error was found. The
Rust code that implements a specification is tested with loom, and the tests
assert the specification's invariants (ADR 18).

## Pros and Cons of the Options

### TLA+ checked by TLC, with tla-ts as a second checker to compare

* Good, because TLC is maintained with the language and is packaged in nixpkgs
  as `tlaplus`.
* Bad, because TLC needs a JVM in the dev shell.

### TLA+ checked by tla-ts alone

* Good, because it needs no JVM and claims to be about twice as fast as TLC.
* Bad, because it is at version 0.0.x and changes daily, so its results need a
  reference to be trusted.

### Quint checked by Apalache

* Good, because Quint reads like a programming language.
* Bad, because Apalache is not in nixpkgs and its maintainers have said it is
  not under active development.

### Interleavings modelled in Lean

* Good, because there would be one language.
* Bad, because Lean has no model checker, so every property would need a
  proof or a hand-written search.

### Testing the Rust code with loom only

* Good, because it checks the real code.
* Bad, because loom explores the interleavings of one test program, which is
  not a specification a reviewer can read.

## More Information

Answers to the questions this record left open:

* The Rust code is held to a specification by loom tests that assert the same
  invariants. Recording traces under loom and checking them with TLC is left
  for later, when a specification is large enough to need it.
* Each specification is checked at the largest constants that TLC finishes in
  about a minute on CI. `SnapshotHandoff` at 15 snapshots and a queue of 3 has
  16 268 states and takes about 2 seconds; at 40 snapshots and a queue of 6,
  TLC had not finished after 20 minutes.

The first comparison of tla-ts 0.0.10 with TLC 1.7.4, on `SnapshotHandoff`
with one worker:

* Both found the same numbers of states, 252 at 6 snapshots and 16 268 at 15.
* Both reported the same violation for each of four broken variants of the
  specification: no room check, a leak, freeing the snapshot being played and
  no fairness for draining the queue.
* tla-ts took 1.1 seconds and 0.2 GB at 15 snapshots, TLC 2.3 seconds and
  0.5 GB, including the start of the JVM.
* At 40 snapshots, tla-ts ran for 9 minutes, used 6.6 GB and stopped without
  a result; why is not known yet.

References:

* [tla-ts](https://github.com/aidenybai/tla-ts), a TLA+ interpreter and model
  checker in TypeScript
* [tla-connect](https://docs.rs/tla-connect), a Rust crate that replays traces
  of a TLA+ specification against an implementation
* [The status of Apalache](https://discuss.tlapl.us/msg06427.html)
