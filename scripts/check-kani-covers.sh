#!/usr/bin/env bash
# Checks a Kani output log to ensure at least one harness was checked and every
# checked harness has at least one cover property with all of them satisfied
# (ADR 11). Kani itself passes a harness whose cover properties are unreachable
# or unsatisfiable.
#
# usage: check-kani-covers.sh LOG_FILE
set -euo pipefail

log=${1:?usage: check-kani-covers.sh LOG_FILE}
prefix=""
if [ -n "${GITHUB_ACTIONS:-}" ]; then
  prefix="::error::"
fi

awk -v prefix="$prefix" '
  /^Checking harness / {
    harness = $3
    sub(/\.\.\.$/, "", harness)
    covered[harness] = 0
    checked++
  }
  /\*\* [0-9]+ of [0-9]+ cover properties satisfied/ {
    for (i = 1; i <= NF; i++) if ($i == "of") { satisfied = $(i - 1) + 0; total = $(i + 1) + 0 }
    covered[harness] = 1
    if (total == 0 || satisfied != total) {
      printf "%s%s: %d of %d cover properties satisfied\n", prefix, harness, satisfied, total
      failed = 1
    }
  }
  END {
    if (checked == 0) {
      printf "%sno Kani harnesses were checked in the log\n", prefix
      exit 1
    }
    for (harness in covered) if (!covered[harness]) {
      printf "%s%s has no cover property\n", prefix, harness
      failed = 1
    }
    exit failed
  }
' "$log"
