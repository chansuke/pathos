---
number: 5
title: Let AI agents operate through the command API without built-in generative models
status: accepted
date: 2026-11-12
---

# Let AI agents operate through the command API without built-in generative models

## Context and Problem Statement

"AI-native" can mean several things for a DAW: an agent that edits the project
with the user, a reviewer of the mix, or models that generate audio and MIDI.
Each leads to a different architecture, so the scope has to be fixed.

## Decision Drivers

* The user stays in control of what changes in the project.
* AI features must be testable with the same harness as the rest of the core.
* The product must not depend on shipping or training generative models.

## Considered Options

* Agents operate the DAW through the command API
* Built-in generative models for audio or MIDI
* AI only as an external bridge or plugin

## Decision Outcome

Chosen option: "Agents operate the DAW through the command API".

An agent reads the project state and proposes or applies commands (ADR 2). The
user can discuss a project with an agent while working on it. pathos does not
include generative audio or MIDI models.

### Consequences

* Good, because agent edits are validated by `step`, shown as a difference and
  undone like any other edit.
* Good, because agents see the whole project, not a single track's audio or a
  remote-control subset.
* Bad, because agent quality depends on how expressive the command vocabulary
  is.
* Bad, because users who want generated material need an external tool.

### Confirmation

Agent integrations call only the command API and read-only queries.

## Pros and Cons of the Options

### Agents operate the DAW through the command API

* Good, because it reuses the specification and tests of the core.
* Bad, because it requires the command API to be complete early.

### Built-in generative models for audio or MIDI

* Good, because it can produce material from a prompt.
* Bad, because it adds model distribution, licensing and evaluation problems
  unrelated to the editor.

### AI only as an external bridge or plugin

* Good, because the DAW itself stays unaware of AI.
* Bad, because a plugin hears only its own track and a bridge sees only what the
  host exposes.
