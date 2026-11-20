---
number: 33
title: Compare with REAPER through a converted project and a JSFX copy of the instrument
status: proposed
date: 2026-11-20
---

# Compare with REAPER through a converted project and a JSFX copy of the instrument

## Context and Problem Statement

ADR 7 decided to render each scenario with pathos and with REAPER and to
compare the results in layers, but not how a pathos project becomes a REAPER
project, nor how REAPER can play notes that only pathos's built-in instrument
(ADR 15) knows how to sound. REAPER's own instruments sound different, so a
comparison with them could not check gain, pan or summing to a threshold. The
instrument also lowers a note over its last 5 ms, which needs the time of the
note-off before it arrives. How are scenarios converted and played in REAPER,
and how are the layers checked?

## Decision Drivers

* The position layer must be exact: every note-on and note-off at the same
  sample (ADR 7, ADR 32).
* The mix layer should fail on any real difference in volume, pan, mute, solo,
  master volume or summing, and not on the rounding of floating point.
* A scenario is a pathos project file (ADR 19), so that any trace can become
  one, including generated ones.
* REAPER's license for CI is not settled (ADR 7), so the comparison must run
  on a developer's machine first, without failing CI where REAPER is missing.

## Considered Options

* A JSFX copy of the instrument, delayed by its fade, in a converted project
* REAPER's ReaSynth in a converted project, compared by measurements only
* MIDI files rendered by REAPER, without a project

## Decision Outcome

Chosen option: "A JSFX copy of the instrument, delayed by its fade, in a
converted project", because it lets REAPER produce the same samples as pathos,
so both layers compare sample by sample.

The crate `pathos-reaper` converts a project into a REAPER project file:

* the tempo changes into a tempo envelope of square points, with the first
  tempo as the project tempo;
* each track into a track with its volume as a linear gain, its pan from -1 to
  1, its mute and its solo, playing the JSFX instrument;
* the clips of each track into one MIDI item from the start of the project, at
  960 ticks a quarter note, with their notes as note-ons and note-offs, a
  note-off before a note-on on the same tick;
* the master volume into the master track's gain; and
* a render of a given number of frames to a 32-bit float WAV file, without
  dither or normalization, with REAPER's pan law at 0 dB.

Two differences from REAPER are deliberate and the converter accounts for them:

* REAPER puts an item's start at a whole sample and the item's notes from
  there, so a note in an item that starts between samples can lie a sample
  from its nearest sample. Items from the start of the project leave each
  note at its nearest sample, as pathos places it (ADR 32).
* REAPER plays a muted track that is soloed, while pathos does not (ADR 21).
  Such a track keeps its solo, which silences the tracks that are not soloed,
  but loses its notes.

The JSFX instrument plays a triangle wave and fades a note in and out over
`max(rate / 200, 1)` frames, as the built-in instrument does, with the same
frequency table, written into it from `pathos-render`. It reports that fade as
its latency and outputs each frame that long after its notes arrive, by which
time it knows whether the note stops within the fade. REAPER compensates the
latency, so the output lies where pathos puts it, and the comparison also
checks the compensation.

The layers:

* position: the instrument outputs an impulse of 1 on the left channel at each
  note-on and on the right at each note-off, with every gain at unity, and the
  impulses must equal the events of the project's schedule at every sample; and
* mix: the sound of the converted project must stay within 1e-4 (-80 dBFS) of
  pathos's render at every sample of both channels.

Scenarios are project files in the crate, plus generated projects. Each runs
at 44.1 kHz and 48 kHz. The tests that run REAPER are ignored by default and
run with `cargo test -p pathos-reaper -- --ignored` in the dev shell
`.#reaper`, which provides REAPER and, on Linux, a virtual display. CI runs
them once a license for CI is settled (ADR 7).

### Consequences

* Good, because positions, volume, pan, mute, solo, the master volume, summing
  and REAPER's latency compensation are compared sample by sample.
* Good, because any project file can be a scenario.
* Bad, because the JSFX instrument copies the built-in instrument and must
  change with it; the mix layer fails until it does.
* Bad, because the instrument's sound itself is not compared with another DAW,
  only its placement and mixing.
* Neutral, because the dev shell `.#reaper` uses REAPER, which is unfree, and
  its 60-day evaluation until a license is bought.

### Confirmation

`cargo test -p pathos-reaper -- --ignored` in the dev shell `.#reaper` renders
every scenario with both and fails when a layer's criterion is not met. The
converter's tests run in CI without REAPER.

## Pros and Cons of the Options

### A JSFX copy of the instrument, delayed by its fade, in a converted project

* Good, because the same samples can be compared, so the criteria are strict.
* Good, because JSFX is text that REAPER compiles itself, so nothing is built
  for REAPER.
* Bad, because the copy has to be kept in step with the instrument.

### REAPER's ReaSynth in a converted project, compared by measurements only

* Good, because it needs no copy of the instrument.
* Bad, because only loudness and onsets could be compared, so a small error in
  gain or a sample of offset would pass.

### MIDI files rendered by REAPER, without a project

* Good, because MIDI files are a standard format.
* Bad, because a MIDI file has no volume, pan, mute, solo or master, so only
  positions could be compared.

## More Information

REAPER 7.86 rendered converted projects at 48 kHz with up to three tracks, two
tempo changes, track volumes and pans, a muted track and a master volume. After
ADR 32, every note-on and note-off lay at the same sample as in pathos, and
the largest difference of a sample was 6.9e-5 with a frequency table computed
in double precision, and 1.8e-7 for notes whose frequency is exact in both.

A generated project found both deliberate differences. Its clip on the second
track started at sample 73201.40625 at 44.1 kHz, and a note-off whose exact
sample is 112592.8125 lay at 112592 in REAPER, the start and the note's offset
in the item each rounded on their own; with the item from the start it lay at
112593. Its first track was muted and soloed, and REAPER played it.

REAPER 7.83, in the nixpkgs of the default dev shell, crashes when it renders
a project, so the dev shell `.#reaper` takes REAPER 7.86 from a newer nixpkgs.
