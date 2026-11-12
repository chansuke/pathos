---
number: 4
title: Gate merges on differential tests and track proofs as a metric
status: accepted
date: 2026-11-12
---

# Gate merges on differential tests and track proofs as a metric

## Context and Problem Statement

The Lean model (ADR 3) is useful only if the Rust core is held to it. Proving
the Rust code itself correct is not practical today, and requiring a finished
proof for every theorem would block ordinary work.

## Decision Drivers

* A change that breaks conformance must not be mergeable.
* The gate must be automatic and fast enough to run on every pull request.
* Unfinished proofs must be visible, never hidden.

## Considered Options

* Differential tests as the gate, proofs tracked as a metric
* Require every theorem to be proved before merging
* Example-based tests only

## Decision Outcome

Chosen option: "Differential tests as the gate, proofs tracked as a metric".

A change may be merged when:

* the model builds, so every definition and theorem statement type-checks; and
* the differential tests pass: generated command sequences are applied to both
  the Rust core and the model's executable, and every result must be equal,
  including errors.

Proofs are not required. A theorem may be stated with `sorry`; the number of
`sorry` occurrences is reported and is a metric to drive down. A failing case
found by the differential tests is kept as a regression test.

### Consequences

* Good, because conformance is checked on behavior the model actually defines,
  with inputs nobody had to think of.
* Good, because agents get numeric targets: mismatches and `sorry` count.
* Bad, because differential tests are sampled evidence, not a proof that the
  Rust core refines the model.
* Bad, because a `sorry` theorem states a property that may be false.

### Confirmation

CI builds the model, runs the differential tests and reports the `sorry` count.

## Pros and Cons of the Options

### Differential tests as the gate, proofs tracked as a metric

* Good, because it is the approach used for Cedar with a Lean model and a Rust
  implementation.
* Bad, because assurance depends on the quality of the input generators.

### Require every theorem to be proved before merging

* Good, because every stated property holds for the model.
* Bad, because it says nothing about the Rust core and slows every change.

### Example-based tests only

* Good, because they are simple.
* Bad, because they only cover the cases someone wrote down.

## More Information

Translating Rust to Lean with [Aeneas](https://github.com/AeneasVerif/aeneas)
could later prove pure parts of the core against the model. It is not part of
this decision.
