---
number: 6
title: Provide AI review as findings with applicable command proposals
status: accepted
date: 2026-11-12
---

# Provide AI review as findings with applicable command proposals

## Context and Problem Statement

Mix feedback today comes from third-party plugins and bridges. A plugin
analyzes only the audio of the track it is inserted on, and most bridges read
session state without hearing anything. pathos can offer review as a built-in
feature that sees both.

## Decision Drivers

* A review should point at a place in the project and be actionable.
* Suggested changes must go through the same validation as any edit.
* Unreleased audio should not leave the machine without the user's consent.

## Considered Options

* Findings anchored to the project, with command proposals
* Free-form chat feedback
* Leave review to third-party plugins

## Decision Outcome

Chosen option: "Findings anchored to the project, with command proposals".

A review takes the project state and measurements computed from a deterministic
offline render, per track and on the master: loudness, true peak, spectrum and
phase correlation. It returns findings, each anchored to a track and a time
range, optionally with a list of commands (ADR 2) that would address it. The
user applies or dismisses each proposal. Review is available by default.

A review may send the render itself, per track and on the master, to an
external service along with the project state and the measurements, since
audio allows a deeper analysis than measurements alone. Audio leaves the
machine only with the user's consent:

* the first review of a project that would send audio asks for consent, and
  names the service and what is sent;
* consent is given per project and can be withdrawn at any time;
* without consent, the review sends only the project state and the
  measurements, and says that its findings are limited by that.

### Consequences

* Good, because a proposal that violates a precondition is rejected by `step`
  before it reaches the project.
* Good, because the measurement functions are pure and can be tested exactly.
* Bad, because the quality of findings cannot be specified formally and needs
  an evaluation set of projects with known problems.
* Bad, because unreleased audio leaves the machine once the user consents, and
  is then subject to the service's terms.
* Neutral, because review depends on the deterministic offline render (ADR 15),
  which exists.

### Confirmation

Tests of review check that no audio is sent for a project without its
consent, and that withdrawing consent stops it.

## Pros and Cons of the Options

### Findings anchored to the project, with command proposals

* Good, because it works like code review, which users of AI tools know.
* Bad, because it needs a schema for findings and anchors.

### Free-form chat feedback

* Good, because it needs no schema.
* Bad, because advice cannot be applied or checked automatically.

### Leave review to third-party plugins

* Good, because it costs nothing to build.
* Bad, because plugins cannot see the whole project.

## More Information

An earlier version of this ADR left open what a review may send to an
external service. Sending only measurements keeps unreleased audio local;
sending audio allows deeper analysis. Audio is sent, with consent given per
project.
