---
number: 12
title: Fingerprint the model's definitions for human review
status: accepted
date: 2026-11-14
links:
- target: 9
  kind: amends
---

# Fingerprint the model's definitions for human review

## Context and Problem Statement

ADR 3 requires a human to review every change to the model's definitions and
theorem statements, since Lean cannot tell whether the model says what was
intended. `theorems.txt` makes a restated theorem visible (ADR 9), but it lists
statements, not the definitions they mention. A pull request that weakens
`Project.wellFormed` or changes a rule of `step` leaves the list unchanged. If
the Rust core changes the same way, the differential tests still agree, and
`step_preserves_wellFormed` is unfinished, so nothing in CI points at the
change. How should a change to a definition be brought to a reviewer?

## Decision Drivers

* A change to what a definition says must be visible in the pull request,
  however small its diff in the source.
* Edits that do not change meaning, such as comments and formatting, must not
  ask for review.
* The check must work from committed lists, without building the model of the
  base commit, as ADR 9's does.

## Considered Options

* List every definition with a fingerprint of its elaborated body
* Require review of every pull request that touches `spec/Pathos/`
* List every definition with its pretty-printed body

## Decision Outcome

Chosen option: "List every definition with a fingerprint of its elaborated
body", because it names exactly the definitions whose meaning changed and
ignores edits that change nothing.

The audit writes `spec/definitions.txt` next to `theorems.txt`, one line per
definition or type written in the source: `name [kind] fingerprint`. The
fingerprint hashes the declaration's type and body, or a type's constructors,
together with the declarations Lean generates for it, such as the matchers of a
`match`. Declarations Lean generates inside a type, such as recursors and
derived instances, are left out; a change to them shows on the type's line.

CI fails when the list is out of date. On a pull request, it compares the list
with the one at the base commit and warns on each definition that changed, was
added or was removed, listing them in the job summary.

This amends ADR 9: the audit now produces two lists, and is run with
`cd spec && lake env lean --run Audit.lean`.

### Consequences

* Good, because a reviewer sees which definitions a pull request changes in
  the diff of `definitions.txt` and in the job summary.
* Good, because comments, formatting and changes to theorems leave the
  fingerprints alone.
* Bad, because a fingerprint says that a definition changed, not how; the
  reviewer still reads the source.
* Bad, because a new Lean version can elaborate a definition differently and
  change fingerprints without any change to the source.
* Neutral, because a change is a warning, not a failure: like a restated
  theorem, it needs review, not rejection.

### Confirmation

On a copy of `spec/`, each of these edits changed the fingerprint of exactly
the definition it touched: dropping the overlap check from `clipsWellFormed`,
changing a comparison inside the `match` of `step`, changing a comparison in
the recursion of `insertTempo`, changing a string literal pattern of
`Request.parse`, and keeping redo in `History.edit`. An edit to a doc comment
changed no fingerprint.

## Pros and Cons of the Options

### List every definition with a fingerprint of its elaborated body

* Good, because it ignores edits that do not change meaning.
* Bad, because the fingerprint depends on how Lean elaborates the source.

### Require review of every pull request that touches `spec/Pathos/`

* Good, because it needs no tooling beyond a code owners file.
* Bad, because pathos has one maintainer, and an approval required of the
  author of a pull request cannot be given.
* Bad, because it does not say which definitions changed.

### List every definition with its pretty-printed body

* Good, because the diff shows how the definition changed.
* Bad, because pretty-printed bodies of `match` and recursive definitions are
  long and change with unrelated elaboration details.
