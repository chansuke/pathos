---
number: 8
title: Scan CI workflows and dependencies for vulnerabilities
status: proposed
date: 2026-11-12
---

# Scan CI workflows and dependencies for vulnerabilities

## Context and Problem Statement

pathos is a public repository. CI runs `cargo deny check`, which reports
RustSec advisories, licenses and sources, but only on pushes and pull
requests, so an advisory published while nothing changes goes unnoticed. The
workflows themselves were not checked: zizmor reported actions referenced by
mutable tags, jobs with default token permissions and checkout credentials
left on the runner. Attacks on CI pipelines and on actions are common, and a
workflow is where a token with write access lives. Which scanners should CI run,
and which should it leave out?

## Decision Drivers

* A compromised action or workflow must not be able to act with more than the
  job needs.
* A new advisory against a locked dependency must be reported without waiting
  for a change.
* Each scanner must find something the others do not; overlapping scanners
  add noise and maintenance.
* Tools come from the Nix dev shell or nixpkgs, as the rest of CI does.
* `Cargo.lock` pins `libc` for gpui (see `README.md`), so nothing may rewrite
  it automatically.

## Considered Options

* One scanner per surface: zizmor, cargo-deny, CodeQL and GitHub's own
  features
* A broad set of general scanners: OSV-Scanner, Trivy, gitleaks and others
* Keep cargo-deny alone

## Decision Outcome

Chosen option: "One scanner per surface", because it covers the workflows, the
dependencies, the code and the secrets each with one tool, and leaves out tools
that would repeat the same advisory databases.

* **Workflows:** every workflow grants its token only what its jobs need,
  read-only access to the repository unless a job must write, pins every
  action to a commit SHA with its tag in a comment, and checks out with
  `persist-credentials: false`. Dependabot waits 7 days
  before proposing a new release of an action. zizmor audits the workflows and
  the Dependabot configuration on every push and pull request, fails on a
  finding and uploads its findings to code scanning.
* **Dependencies:** cargo-deny keeps checking advisories, licenses and sources
  on every push and pull request, and also checks advisories on a daily
  schedule. Dependabot alerts compare `Cargo.lock` with the GitHub Advisory
  Database. Dependabot security updates stay off, because they would rewrite
  `Cargo.lock`.
* **Code:** CodeQL scans Rust and the workflows with the default setup.
* **Secrets:** GitHub secret scanning and push protection are enabled.

Left out:

* OSV-Scanner, because for `Cargo.lock` it reads the same RustSec and GitHub
  advisories as cargo-deny and Dependabot alerts.
* Trivy and Grype, because pathos publishes no container image.
* gitleaks, because push protection blocks secrets before they are pushed.
* Scanning the flake inputs, because pathos is not distributed as a Nix
  package and those scanners report many false positives.

### Consequences

* Good, because a compromised action cannot be swapped in under an existing
  tag, and a job's token cannot write to the repository unless the job needs
  it.
* Good, because advisories are reported within a day even when nothing
  changes.
* Good, because findings from zizmor and CodeQL appear on the pull request and
  in one code scanning view.
* Bad, because updating an action means updating a SHA, which only Dependabot
  does comfortably.
* Bad, because a new advisory can fail a pull request that did not touch the
  affected dependency.
* Bad, because repository settings (CodeQL default setup, secret scanning,
  Dependabot alerts) are not recorded in the repository.

### Confirmation

The zizmor workflow fails on any finding, including an unpinned action or a
job without a `permissions` block. The scheduled cargo-deny job fails on an
advisory. The repository settings are checked with
`gh api repos/chansuke/pathos --jq .security_and_analysis`.

## Pros and Cons of the Options

### One scanner per surface: zizmor, cargo-deny, CodeQL and GitHub's own features

* Good, because zizmor targets GitHub Actions specifically and also audits the
  Dependabot configuration.
* Good, because cargo-deny is already in the dev shell and CI.
* Good, because CodeQL scans Rust without a build, so it does not need the Nix
  dev shell.
* Neutral, because CodeQL's workflow queries overlap with zizmor in part, but
  they use data flow analysis that zizmor does not.
* Bad, because no single workflow scanner covers every weakness; a 2026 study
  of nine scanners found none that did.

### A broad set of general scanners: OSV-Scanner, Trivy, gitleaks and others

* Good, because more advisory sources and detectors are consulted.
* Bad, because for this repository they repeat what cargo-deny, Dependabot
  alerts and push protection report.
* Bad, because every additional action is another dependency of CI that can
  itself be compromised.

### Keep cargo-deny alone

* Good, because nothing changes.
* Bad, because the workflows stay unaudited and advisories are only found on a
  change.

## More Information

* zizmor audits: <https://docs.zizmor.sh/audits/>
* CodeQL Rust support is generally available:
  <https://github.blog/changelog/2025-10-14-codeql-scanning-rust-and-c-c-without-builds-is-now-generally-available>
* Unpacking Security Scanners for GitHub Actions Workflows:
  <https://arxiv.org/abs/2601.14455>

The workflow hardening and zizmor land first; the scheduled advisory check,
CodeQL and the repository settings follow.
