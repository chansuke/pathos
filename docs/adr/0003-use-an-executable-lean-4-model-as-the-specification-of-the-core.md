---
number: 3
title: Use an executable Lean 4 model as the specification of the core
status: accepted
date: 2026-11-12
---

# Use an executable Lean 4 model as the specification of the core

## Context and Problem Statement

pathos is developed specification first, largely by AI agents. Agents need an
oracle that says whether the core behaves correctly, and humans need a
specification small enough to review instead of reading all generated code.
Natural-language specifications drift from the implementation and cannot be
executed.

## Decision Drivers

* The specification must be machine-checked and executable.
* There must be a single source of truth for the core's behavior.
* The specification must build quickly so that it fits in an agent's loop.

## Considered Options

* An executable Lean 4 model as the specification
* Markdown specifications managed by a spec-driven development tool
* A typed specification IR, such as Pkl, with Lean for selected fragments

## Decision Outcome

Chosen option: "An executable Lean 4 model as the specification".

The model in `spec/` defines the project state, the commands (ADR 2) and
`step`, together with theorems about them. It is normative for the project
core: when the model and the Rust core disagree, the core is wrong unless the
model is changed first. The model uses no mathlib and no other dependencies.
Requirements in prose live as doc comments in the model and as ADRs, and have
no force on their own.

Parts that are not modelled in Lean use other oracles: the hand-off between the
UI and audio threads, real-time safety, DSP and the UI.

### Consequences

* Good, because the same artifact is read by humans, proved about and run as a
  test oracle.
* Good, because there is no second representation of the transition function
  that could drift from the first.
* Bad, because Lean cannot tell whether the model says what was intended, so a
  human has to review changes to definitions and theorem statements.
* Bad, because contributors need to read Lean.

### Confirmation

`lake build` in `spec/` succeeds, and the Rust core is checked against the
model's executable as described in ADR 4.

## Pros and Cons of the Options

### An executable Lean 4 model as the specification

* Good, because Lean 4 is both a programming language and a proof assistant.
* Bad, because proofs can be expensive to write.

### Markdown specifications managed by a spec-driven development tool

* Good, because anyone can read and write them.
* Bad, because they cannot be executed and drift from the code.

### A typed specification IR, such as Pkl, with Lean for selected fragments

* Good, because the authoring surface is approachable and localizable.
* Bad, because the transition function exists twice and their correspondence
  has to be maintained.

## More Information

* [How We Built Cedar: A Verification-Guided Approach](https://arxiv.org/abs/2407.01688)
* [dspec](https://github.com/mizchi/dspec) and
  [docs2formalspec](https://github.com/NyxFoundation/docs2formalspec), whose
  ideas on traceability and honest metrics informed this decision
