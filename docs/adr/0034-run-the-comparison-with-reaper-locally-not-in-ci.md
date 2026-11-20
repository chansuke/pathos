---
number: 34
title: Run the comparison with REAPER locally, not in CI
status: proposed
date: 2026-11-20
---

# Run the comparison with REAPER locally, not in CI

## Context and Problem Statement

ADR 7 decided that CI renders the scenarios with REAPER on every change, and
ADR 33 left the workflow until a license for CI was settled. REAPER's license
allows one computer at a time and says nothing about CI runners, automated
use or keeping the license key in a CI service, so running it on GitHub's
runners needs Cockos to confirm. A self-hosted runner would keep REAPER and
the key on the developer's own computer, but in a public repository it can run
code from pull requests on that computer, keeps state between jobs, and must
be kept running and updated. Where does the comparison run?

## Decision Drivers

* REAPER must be used within its license, without relying on an unconfirmed
  reading of it.
* Code from outside contributors must not run on a developer's computer.
* A change that moves a note or changes the mix should not reach `main`
  unnoticed.

## Considered Options

* Run the comparison locally before a pull request
* Run it in CI on GitHub's runners, after Cockos confirms
* Run it in CI on a self-hosted runner

## Decision Outcome

Chosen option: "Run the comparison locally before a pull request", because it
uses REAPER on one computer, the developer's own, and runs no code but the
developer's.

This replaces the part of ADR 7 that runs REAPER in CI. CI keeps testing the
conversion without REAPER (ADR 33). A pull request that changes
`crates/pathos-core`, `crates/pathos-render` or `crates/pathos-reaper` runs
`nix develop .#reaper -c cargo test -p pathos-reaper -- --ignored` before it is
opened, and states the result in its description.

Two checks keep the comparison from being forgotten:

* a pre-push hook in `.githooks`, which the dev shell enables with
  `core.hooksPath`, runs the comparison before a push that changes those
  crates, and remembers the code that has passed so as not to run it again;
  and
* a CI job fails a pull request that changes those crates unless its
  description has the line `REAPER comparison: passed`.

The tests run one REAPER at a time: two at once sometimes left a WAV file cut
short, and the license allows one computer at a time.

### Consequences

* Good, because REAPER runs only where its license clearly allows.
* Good, because no runner executes code from pull requests on a personal
  computer.
* Bad, because the checks rely on honesty: `git push --no-verify` skips the
  hook, a clone outside the dev shell has no hook, a branch updated on GitHub
  is not compared, and the line in the description is not proof of a run.
* Bad, because the comparison runs only on Linux, where the dev shell `.#reaper`
  exists.
* Neutral, because the decision is revisited if Cockos confirms that CI may
  use a license, or if a REAPER license is bought for CI.

### Confirmation

`AGENTS.md` names the command and when to run it, the hook runs it, and the
CI job checks that each such pull request states its result.

## Pros and Cons of the Options

### Run the comparison locally before a pull request

* Good, because it needs nothing beyond the dev shell `.#reaper`.
* Bad, because it depends on discipline rather than CI.

### Run it in CI on GitHub's runners, after Cockos confirms

* Good, because every change is compared.
* Bad, because it waits on Cockos and needs the license key in a CI secret.

### Run it in CI on a self-hosted runner

* Good, because every change is compared and the key stays on the developer's
  computer.
* Bad, because code from pull requests could run on that computer if the
  runner is misconfigured.
* Bad, because the runner keeps state between jobs and must be kept running
  and updated.

## More Information

REAPER's license: <https://www.reaper.fm/license.txt>, sections 1.2 to 1.6.
