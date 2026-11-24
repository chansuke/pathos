#!/bin/sh
# Checks that a pull request that changes the core, the render or the
# converter states a passing comparison with REAPER for its own head (ADR 34).
# The description is read from $BODY.
#
# usage: check-reaper-line.sh BASE HEAD
set -eu

base=${1:?usage: check-reaper-line.sh BASE HEAD}
head=${2:?usage: check-reaper-line.sh BASE HEAD}
paths="crates/pathos-core crates/pathos-render crates/pathos-reaper"
# $paths is split into one argument per path on purpose.
# shellcheck disable=SC2086 # one argument per path
if git diff --quiet "$base...$head" -- $paths; then
  echo "The pull request does not change $paths."
  exit 0
fi
key=$("$(dirname "$0")/reaper-key" "$head")
stated=$(printf '%s\n' "${BODY:-}" | tr -d '\r' | sed -n 's/^REAPER comparison: passed *//p' | tail -n 1)
if [ "$stated" = "$key" ]; then
  echo "The pull request states that the comparison with REAPER passed on its head."
elif [ -z "$stated" ] && ! printf '%s\n' "${BODY:-}" | tr -d '\r' | grep -q '^REAPER comparison: passed'; then
  echo "::error::This pull request changes $paths. Push it from a clean checkout of its head on Linux, so that the pre-push hook compares it with REAPER, and put the line the hook prints, 'REAPER comparison: passed $key', in its description (ADR 34)."
  exit 1
else
  echo "::error::The description states a comparison with REAPER of other code (${stated:-no key}), not of this head ($key). Check out the head without changes and push again; the hook compares it and prints the line to put in the description (ADR 34)."
  exit 1
fi
