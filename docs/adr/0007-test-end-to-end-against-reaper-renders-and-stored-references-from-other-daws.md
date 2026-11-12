---
number: 7
title: Test end to end against REAPER renders and stored references from other DAWs
status: accepted
date: 2026-11-12
---

# Test end to end against REAPER renders and stored references from other DAWs

## Context and Problem Statement

The Lean model (ADR 3) checks that the core is consistent with itself, but not
that it models what a DAW should do. pathos aims to be comparable to Ableton
Live, Logic Pro and FL Studio, so its end-to-end tests should compare its output
with established DAWs. Those three cannot run in CI, have no shared project
format, and none is available to the project today.

## Decision Drivers

* A project moved to pathos should sound the same; fidelity comes before
  quality comparisons.
* The comparison must run automatically on every change.
* Scenarios must be cheap to add, including generated ones.
* Other DAWs are evidence, not the specification.

## Considered Options

* REAPER rendered in CI, plus stored reference renders from other DAWs
* Stored reference renders only, made by hand in each DAW
* pathos's own golden renders only

## Decision Outcome

Chosen option: "REAPER rendered in CI, plus stored reference renders from other
DAWs".

A scenario is a command trace (ADR 2) with its audio assets and the
measurements expected of the result. It is rendered by pathos's deterministic
offline render and converted to a REAPER project, which REAPER renders in CI
with `-renderproject`. Settings that differ between DAWs, such as the pan law,
are stated in the scenario.

Renders are compared in layers:

* position: the sample position of impulses, across tempo changes, clip edits
  and latency compensation, which must match exactly;
* mix: gain, pan, summing, fades and automation, where the difference must stay
  below a threshold;
* quality: operations with no single right answer, such as sample rate
  conversion, where measurements must be no worse than the reference; and
* interchange: reading and writing MIDI files and DAWproject, compared by
  structure.

A difference from REAPER is either a defect in pathos or a deliberate
difference, and a deliberate one is recorded in an ADR. Reference renders from
Ableton Live, Logic Pro and FL Studio are stored with the DAW version and
settings that produced them, and added as they become available. Once several
references exist, a scenario on which they agree is binding for pathos.

UI tests stay thin and check which command a UI action produces. Load and track
count comparisons are manual benchmarks, not CI tests.

### Consequences

* Good, because the model's notion of time and mixing is checked against a DAW
  people already rely on.
* Good, because scenarios are generated, so the comparison can cover projects
  nobody wrote by hand.
* Bad, because with a single reference, REAPER's own quirks cannot be told
  apart from behavior common to DAWs.
* Bad, because proprietary processing, such as time stretching and built-in
  effects, cannot be compared this way.
* Bad, because CI depends on a proprietary program and its license.

### Confirmation

CI renders the scenarios with pathos and REAPER and fails when a layer's
criterion is not met.

## Pros and Cons of the Options

### REAPER rendered in CI, plus stored reference renders from other DAWs

* Good, because REAPER has a command-line render, a Linux build and a text
  project format.
* Bad, because the converter to REAPER projects has to be maintained.

### Stored reference renders only, made by hand in each DAW

* Good, because it compares with the DAWs named as the goal.
* Bad, because each scenario is manual work in every DAW, and none of them is
  available now.

### pathos's own golden renders only

* Good, because it needs nothing external.
* Bad, because it only detects change, not whether the output was right.

## More Information

Open items:

* Whether REAPER's license permits automated use in CI has not been confirmed.
  Its EULA has to be read before the workflow is added.
* The deterministic offline render does not exist yet and comes first.
* gpui's screenshot testing has not been tried with the gpui version in use.
