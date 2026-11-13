---
number: 11
title: Check the Rust core with Kani for bounded inputs
status: proposed
date: 2026-11-13
---

# Check the Rust core with Kani for bounded inputs

## Context and Problem Statement

The Lean model is proved and the Rust core is compared with it on generated
traces (ADR 3, ADR 4). The traces sample values; they do not show that the Rust
code never overflows a `u64` or panics for some value nobody generated, and the
model's `Nat` has no overflow at all. Which tool should check the Rust code
itself for every value of its inputs?

## Decision Drivers

* The check must run on the Rust code as written, without a second
  implementation or a rewrite of the core.
* It must cover arithmetic overflow and panics for every `u64` value.
* It must run in CI on a standard runner, and must not take a developer's
  machine down.

## Considered Options

* Kani, with harnesses on projects of fixed size
* Verus
* Aeneas with proofs in Lean
* Keep the differential tests alone

## Decision Outcome

Chosen option: "Kani, with harnesses on projects of fixed size", because it
checks the existing code for every value of its inputs with no change to the
code beyond the harnesses.

Harnesses live in `crates/pathos-core/src/project/verification.rs`, compiled
only under `cfg(kani)`. Each one builds a well-formed project with a fixed
number of tempo changes and clips, from any values, and applies one kind of
command with any arguments. It asserts what the model states as
`step_preserves_wellFormed` and `step_nextClipId_le`: the command fails, or the
result is well-formed and the next clip id has not decreased. Kani also fails
the harness on any overflow or panic. Another harness checks that
`tick_to_micros` does not overflow for any tick.

A harness whose assumptions no input satisfies passes without checking
anything. Each harness therefore has a cover property, for example that some
input reaches an accepted command. Kani does not fail a harness whose cover is
unsatisfiable or unreachable, so CI does.

The harnesses assume that the track count and the next clip id are below
`u64::MAX`, since each increment takes one accepted edit. What happens when
they are reached is not specified by the model yet.

Kani runs in its own CI workflow, outside the Nix dev shell: it is not in
nixpkgs, and `cargo kani setup` installs the nightly it needs with rustup. The
workflow pins the Kani version; Dependabot does not update it.

### Consequences

* Good, because the Rust core is checked for every value of the inputs of each
  harness, overflow included.
* Good, because a harness states the model's property against the Rust code,
  which the differential tests only sample.
* Bad, because the guarantee is bounded: projects of at most two tempo changes
  or two clips, one command at a time.
* Bad, because the solver's time and memory grow quickly with the size of a
  harness. Run locally, one solver per harness in parallel, an earlier set of
  harnesses used 55 GB and the desktop session was killed for memory
  pressure. Harnesses must stay small, and CI runs them one at a time.
* Bad, because comparing two products, as monotonicity of `tick_to_micros`
  needs, is hard for the solver: a harness for it did not finish in five
  minutes. That property is left to the model's `tickToMicros_monotone`.

### Confirmation

The Kani workflow runs every harness on each pull request and on `main`, writes
the verdict of each harness to the job summary, and fails when a harness fails
or does not satisfy all of its cover properties.

## Pros and Cons of the Options

### Kani, with harnesses on projects of fixed size

* Good, because it works on unmodified Rust and checks overflow by default.
* Bad, because it is bounded, and its cost depends heavily on the shape of the
  harness.

### Verus

* Good, because it proves functional properties for unbounded inputs.
* Bad, because the code must be written inside `verus!` with its own toolchain,
  and its specification would duplicate the Lean model.

### Aeneas with proofs in Lean

* Good, because proofs would be against the existing Lean model.
* Bad, because whether it extracts this code with Rust 1.99 is unknown. It
  remains the next step to try for small, pure functions.

### Keep the differential tests alone

* Good, because nothing changes.
* Bad, because overflow and panics for values outside the generators go
  unchecked.
