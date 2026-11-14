---
number: 13
title: Measure the tests by mutating the Rust core
status: proposed
date: 2026-11-14
---

# Measure the tests by mutating the Rust core

## Context and Problem Statement

The differential tests are the merge gate (ADR 4), and
`generators_reach_every_rule` shows that the generators reach every error and
every kind of request. Neither shows that the tests notice a wrong rule: a
generator can reach a rule often and still never produce the input on which an
off-by-one in the Rust core differs from the model. How should pathos measure
whether the tests catch a defect in the core?

## Decision Drivers

* The measure must exercise the real tests against the real core.
* A change to the core must be measured before it merges, at a cost that fits
  in a pull request.
* A defect the tests cannot catch must be visible, not averaged into a score.

## Considered Options

* cargo-mutants: on the lines a pull request changes, and weekly on the whole
  core
* cargo-mutants weekly only
* Hand-written mutations, checked once

## Decision Outcome

Chosen option: "cargo-mutants on the lines a pull request changes, and weekly
on the whole core", because it measures each change when it is made and
catches tests that weaken later.

The Mutants workflow builds the model and runs `cargo mutants` on
`pathos-core` with the tests of `pathos-core`. On a pull request it mutates only
the lines of `crates/pathos-core` that the pull request changes; on Mondays it
mutates the whole crate. It fails when a mutant is missed or times out and
lists the missed mutants in the job summary.

A missed mutant is a gap in the tests, to be closed with a test or a better
generator. It is excluded in `.cargo/mutants.toml` only when no test can catch
it, with the reason in a comment: the Kani harnesses, which build only under
`cfg(kani)`, and equivalent mutants, which do not change behavior.

The measurement also keeps the lists in the tests complete: `ERRORS` and
`REQUEST_KINDS` are generated from exhaustive matches, and each command must
name its Kani harnesses, so a new error, request or command does not compile
until the tests account for it.

### Consequences

* Good, because a pull request that adds a rule the tests do not catch fails.
* Good, because the weekly run notices tests weakened by changes outside the
  mutated lines, such as a narrower generator.
* Bad, because the whole crate takes about ten minutes, since every mutant
  runs the differential tests.
* Bad, because equivalent mutants have to be found and excluded by hand.

### Confirmation

On the whole crate, cargo-mutants caught 82 mutants and missed 50. 48 of the
missed were in the Kani harnesses, which the tests never build. The other two
were the `Display` of `EditError`, which no test checked and which
`every_error_has_a_message` now does, and an equivalent mutant in
`tick_to_micros`. With both excluded or caught, every remaining mutant of those
functions is caught.

## Pros and Cons of the Options

### cargo-mutants on the lines a pull request changes, and weekly on the whole core

* Good, because a change is measured before it merges.
* Bad, because a pull request that changes many lines of the core waits longer.

### cargo-mutants weekly only

* Good, because pull requests do not wait for it.
* Bad, because a gap is found after the change that opened it has merged.

### Hand-written mutations, checked once

* Good, because it needs no tool.
* Bad, because it covers only the mutations someone thought of, once.
