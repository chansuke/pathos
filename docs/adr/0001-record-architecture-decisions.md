---
number: 1
title: Record architecture decisions
status: accepted
date: 2026-11-11
---

# Record architecture decisions

## Context and Problem Statement

pathos is a DAW built with gpui. Choices such as the audio backend, the
threading model between the UI and the audio engine, and the project file
format are costly to revisit. We need a place to record why each choice was
made.

## Decision Drivers

* Keep the reasoning behind a decision next to the code it affects.
* Make decisions reviewable in pull requests like any other change.

## Considered Options

* Architecture Decision Records in `docs/adr/`, managed with `adrs`
* Commit messages and pull request descriptions only

## Decision Outcome

Chosen option: "Architecture Decision Records in `docs/adr/`, managed with
`adrs`", because the records stay in the repository and are reviewed with the
change that introduces them.

ADRs use the MADR format with YAML frontmatter, as configured in `adrs.toml`.
Create one with `adrs new "<title>"` and regenerate the index with
`adrs generate toc > docs/adr/README.md`. `adrs` is available in `nix develop`.

### Consequences

* Good, because a decision and its alternatives can be found without digging
  through history.
* Bad, because each significant decision takes an extra file to write.

### Confirmation

`adrs doctor` reports no errors, and `docs/adr/README.md` lists every ADR.

## Pros and Cons of the Options

### Architecture Decision Records in `docs/adr/`, managed with `adrs`

* Good, because each decision has a stable number that code and other ADRs can
  reference.
* Good, because `adrs` tracks status and links between records.
* Bad, because it adds a tool to the development environment.

### Commit messages and pull request descriptions only

* Good, because it needs no extra files or tools.
* Bad, because the reasoning is scattered and hard to find later.

## More Information

* [Documenting Architecture Decisions](https://www.cognitect.com/blog/2011/11/15/documenting-architecture-decisions)
  by Michael Nygard
* [MADR](https://adr.github.io/madr/)
* [adrs](https://github.com/joshrotenberg/adrs)
