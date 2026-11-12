---
number: 2
title: Share one command vocabulary across the model, core, UI and agents
status: accepted
date: 2026-11-12
---

# Share one command vocabulary across the model, core, UI and agents

## Context and Problem Statement

pathos aims to be an AI-native DAW comparable to established ones. Edits arrive
from the UI, from undo and redo, from AI agents and from generated tests. If
each path changes the project in its own way, they drift apart and no single
specification covers them.

## Decision Drivers

* One definition of what an edit is, for humans, agents and tests alike.
* The specification must be executable so that implementations can be checked
  against it.
* Agent edits must be reviewable and reversible before the user accepts them.

## Considered Options

* One command vocabulary applied by a pure `step` function
* Direct mutation of project state from each caller
* A separate scripting or remote-control API for agents

## Decision Outcome

Chosen option: "One command vocabulary applied by a pure `step` function".

The project core is a state machine: `step : Project → Command → Result
Project`. A `Command` is the unit of UI editing, of undo, of an agent tool call
and of a generated test input. Nothing changes the project except by applying a
command.

### Consequences

* Good, because the Lean model (ADR 3), the Rust core, the UI and agents share
  one vocabulary, so a specification of `step` covers all of them.
* Good, because an agent's proposal is a list of commands that can be
  validated, auditioned, accepted or discarded, and undone like any other edit.
* Bad, because every new editing feature has to be expressed as a command
  before any caller can use it.
* Bad, because real-time state on the audio thread is outside this model and
  needs its own specification.

### Confirmation

The core crate exposes no way to change a `Project` other than applying a
`Command`. The differential tests (ADR 4) generate inputs from the same
`Command` type.

## Pros and Cons of the Options

### One command vocabulary applied by a pure `step` function

* Good, because a pure function is straightforward to model, test and prove
  properties about.
* Bad, because large edits need care to stay cheap when the state is treated as
  a value.

### Direct mutation of project state from each caller

* Good, because it is the least code at first.
* Bad, because undo, agents and tests each need their own view of what changed.

### A separate scripting or remote-control API for agents

* Good, because it can be added to an existing design.
* Bad, because agents see a narrower or different project than the UI does,
  which is the limitation of bridges bolted onto existing DAWs.
