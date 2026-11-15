---
number: 15
title: Render offline from a modelled note schedule and a built-in instrument
status: accepted
date: 2026-11-15
---

# Render offline from a modelled note schedule and a built-in instrument

## Context and Problem Statement

AI review (ADR 6) measures a deterministic offline render, and the end-to-end
tests (ADR 7) compare one with REAPER; both say the render comes first. ADR 3
leaves DSP outside the Lean model. With MIDI clips (ADR 14), a render has two
parts: deciding at which sample each note starts and stops, and turning notes
into sound. Which part does the model specify, and how is the rest checked?

## Decision Drivers

* The sample at which a note starts must be exact, because ADR 7 requires
  positions to match exactly and every later measurement depends on them.
* A render must be identical, bit for bit, every time and on every platform,
  so that a stored render or measurement means something.
* The model stays fast enough for an agent's loop (ADR 3), so it cannot compute
  millions of samples.
* What a user hears while playing must later be what the offline render
  produces.

## Considered Options

* A modelled note schedule, with synthesis in Rust
* The whole render in Rust, checked against stored renders and REAPER
* The whole render in the model, synthesis included

## Decision Outcome

Chosen option: "A modelled note schedule, with synthesis in Rust", because the
part that has one right answer is specified exactly, and the part that does not
is kept small and deterministic.

The model defines a schedule: for a project and a sample rate in hertz, the list
of note-on and note-off events of every clip, each with its sample, track, pitch
and velocity. The sample of a tick is the time of the tick times the sample
rate, rounded down, computed from `microsTimesPpq` without rounding in between.
Events are ordered by sample, then note-off before note-on, then track, then
pitch. The differential tests compare the Rust schedule with the model's, as
they compare `step`.

A new crate, `pathos-render`, turns a schedule into samples. It is a pure
function from a project, a sample rate and a number of frames to the samples,
with no clock, thread or device. Each note plays a built-in instrument: one
oscillator with a short linear attack and release, so that a note starts and
stops without a click. Tracks are summed with equal weight into one channel
until the project has a mixer. The arithmetic uses only addition, subtraction,
multiplication and division of `f32`, which IEEE 754 makes exact to the bit, and
no function of the platform's math library, whose results may differ.

The instrument is fixed signal processing, not a generative model (ADR 5).
Plugin hosting and real-time playback are left to later decisions; playback is
to call the same synthesis code.

### Consequences

* Good, because the position layer of ADR 7 is checked against the model on
  every pull request, before any REAPER workflow exists.
* Good, because the synthesis can be tested exactly: silence where no note
  sounds, an onset at the scheduled sample, and the same output on every run.
* Bad, because the sample of a tick needs a product of up to about 2^40 ticks,
  6·10^7 microseconds and the sample rate, so the Rust core computes it in
  `u128`.
* Bad, because the built-in instrument sounds unlike REAPER's ReaSynth, so only
  the position layer of ADR 7 applies to MIDI; the mix and quality layers wait
  for audio clips.
* Neutral, because the mono sum lasts only until the mixer replaces it.

### Confirmation

The differential tests compare the schedule for generated projects and sample
rates. Tests of `pathos-render` check silence, onsets and determinism, and
compare the hash of a stored scenario's render, which CI checks on each
platform it builds.

## Pros and Cons of the Options

### A modelled note schedule, with synthesis in Rust

* Good, because the model stays a list of events, cheap to compute and to
  reason about.
* Bad, because the schedule exists in both the model and the core, held
  together by the differential tests.

### The whole render in Rust, checked against stored renders and REAPER

* Good, because there is one implementation.
* Bad, because a stored render only detects change, and no REAPER workflow
  exists yet (ADR 7).

### The whole render in the model, synthesis included

* Good, because every sample would be specified.
* Bad, because Lean's `Float` is not specified by the model's theorems and
  computing millions of samples would make the model slow.

## More Information

Open question: the accepted range of sample rates. The schedule is defined for
any positive rate; the render may limit it, for example to 8 000 to 192 000 Hz.
