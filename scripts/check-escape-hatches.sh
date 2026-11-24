#!/usr/bin/env bash
# Rejects changes that put off a fix or switch a check off without a reason:
# TODO-style markers and "workaround" comments, `#[ignore]` and shellcheck
# directives without a reason, a lower minimum or number of cases for the
# generated tests, removed proptest seeds, and an exclusion of cargo-mutants
# without the reason above it. Only the lines a change adds or replaces are
# read, so existing code never fails it.
#
# usage: check-escape-hatches.sh --staged     (pre-commit: the index against HEAD)
#        check-escape-hatches.sh BASE [HEAD]  (CI: BASE to HEAD, or to the working tree)
set -euo pipefail

case "${1:-}" in
--staged)
  diff=(git diff --cached --no-color --no-ext-diff -U0 --)
  show() { git show ":$1"; }
  ;;
"")
  echo "usage: $0 --staged | BASE [HEAD]" >&2
  exit 2
  ;;
*)
  diff=(git diff --no-color --no-ext-diff -U0 "$@" --)
  if (($# > 1)); then
    head=$2
    show() { git show "$head:$1"; }
  else
    show() { cat "$1"; }
  fi
  ;;
esac

# Each rule: a file pattern (a shell glob in which `*` also matches `/`), an
# extended regular expression for the added text, and what to do instead.
# Comment rules read only the comment part of a line; see `comment_of`.
rules=(
  $'comment\t*\t(^|[^A-Za-z])(TODO|FIXME|HACK|XXX)([^A-Za-z]|$)\tFix it in this change. If it is out of scope, open an issue and describe what the code does now as a fact, without a promise; better, add a test or a lint that fails until it is fixed.'
  $'comment\t*\t[Ww]ork-?around|[Ff]or now|[Qq]uick fix|[Bb]and-?aid|[Tt]emporary (fix|hack|patch)|[Uu]ntil .* (is|gets|are) fixed\tDo not explain away a workaround. Find the cause and fix it; if the cause is outside this repository, say which upstream behavior the code handles and link the upstream issue.'
  $'code\t*.rs\t#\\[ignore\\]\tGive the reason: #[ignore = "why it does not run by default"], as the REAPER tests do. A test that fails is fixed, not ignored.'
  $'code\t*.sh\t#[[:space:]]*shellcheck[[:space:]]+disable=[A-Z0-9,]+[[:space:]]*$\tSay why after the directive: `# shellcheck disable=SC2086 # the paths are split on purpose`.'
  $'code\t.githooks/*\t#[[:space:]]*shellcheck[[:space:]]+disable=[A-Z0-9,]+[[:space:]]*$\tSay why after the directive: `# shellcheck disable=SC2086 # the paths are split on purpose`.'
)

# The comment part of LINE in FILE, empty when it has none. Rust uses `//`,
# Lean `--`, and the others `#`; a `#` that starts an attribute such as
# `#[cfg]` is code.
comment_of() { # FILE LINE
  case $1 in
  *.rs) [[ $2 == *//* ]] && printf '%s' "//${2#*//}" ;;
  *.lean) [[ $2 == *--* ]] && printf '%s' "--${2#*--}" ;;
  *.sh | *.toml | *.yaml | *.yml | *.nix | *.tla | *.cfg | .githooks/*)
    [[ $2 =~ (^|[[:space:]])#([^[!]|$) ]] && printf '%s' "#${2#*#}"
    ;;
  esac
  return 0
}

# Text files only: prose in docs may say "for now", and the seeds are data.
checked() { # FILE
  case $1 in
  docs/* | *.md | *.lock | *.proptest-regressions | spec/theorems.txt | spec/definitions.txt) return 1 ;;
  scripts/check-escape-hatches.sh | scripts/test-check-escape-hatches.sh) return 1 ;;
  esac
  return 0
}

# Settings of the tests and the workflows that only go up: a minimum the
# generators must reach and the number of generated cases. Each is an
# extended regular expression whose first group is the number.
thresholds=(
  'MIN_HITS[^=]*=[[:space:]]*([0-9_]+)'
  'COVERAGE_TRACES[^=]*=[[:space:]]*([0-9_]+)'
  'with_cases\(([0-9_]+)\)'
  'PROPTEST_CASES:[[:space:]]*([0-9_]+)'
  'unwrap_or\(([0-9_]+)\)'
)
threshold_advice='Do not lower a minimum or a number of cases to make a test pass. Make the generators reach the rule more often, or fix the code; the minimums are what ADR 4 and ADR 13 rely on.'

found=0
report() { # FILE LINE TEXT ADVICE
  printf '%s:%s: %s\n    %s\n' "$1" "$2" "${3#"${3%%[![:space:]]*}"}" "$4" >&2
  found=$((found + 1))
}

# The value of each threshold on the lines a hunk removes, by pattern, so the
# added lines of the hunk can be compared with them.
declare -A removed=()
file=""
line=0
while IFS= read -r row; do
  case $row in
  '+++ b/'*)
    file=${row#+++ b/}
    continue
    ;;
  '+++ '* | '--- '*) continue ;;
  '@@ '*)
    # @@ -a,b +c,d @@: the next added line is line c of the new file.
    line=${row#*+}
    line=${line%%[, ]*}
    removed=()
    continue
    ;;
  '-'*)
    text=${row#-}
    if [[ $file == *.proptest-regressions && $text == cc\ * ]]; then
      report "$file" "$line" "$text" "Keep each seed that once failed, so the case runs again on every test run."
    fi
    for pattern in "${thresholds[@]}"; do
      if [[ $text =~ $pattern ]]; then
        removed[$pattern]=${BASH_REMATCH[1]//_/}
      fi
    done
    continue
    ;;
  '+'*) ;;
  *) continue ;;
  esac
  text=${row#+}
  if checked "$file"; then
    comment=$(comment_of "$file" "$text")
    for rule in "${rules[@]}"; do
      IFS=$'\t' read -r kind glob pattern advice <<<"$rule"
      # shellcheck disable=SC2053 # the pattern is meant to glob
      [[ $file == $glob ]] || continue
      subject=$text
      [ "$kind" = comment ] && subject=$comment
      if [ -n "$subject" ] && grep -Eq -- "$pattern" <<<"$subject"; then
        report "$file" "$line" "$text" "$advice"
      fi
    done
    for pattern in "${thresholds[@]}"; do
      case $file in
      */tests/* | .github/workflows/*) ;;
      *) continue ;;
      esac
      if [[ $text =~ $pattern ]] && [ -n "${removed[$pattern]:-}" ] &&
        ((${BASH_REMATCH[1]//_/} < ${removed[$pattern]})); then
        report "$file" "$line" "$text" "$threshold_advice"
      fi
    done
  fi
  line=$((line + 1))
done < <("${diff[@]}")

# Every exclusion of cargo-mutants has its reason in the comment above it
# (ADR 13). The whole file is read, since an entry may move.
mutants=.cargo/mutants.toml
if "${diff[@]}" "$mutants" | grep -q '^+'; then
  number=0
  previous=""
  in_list=false
  while IFS= read -r text; do
    number=$((number + 1))
    case $text in
    *'= ['*) in_list=true ;;
    ']'*) in_list=false ;;
    *\"*)
      if $in_list && [[ $previous != *\"* ]] && [[ ! $previous =~ ^[[:space:]]*# ]]; then
        report "$mutants" "$number" "$text" "Say in a comment above the entry why no test can catch this mutant (ADR 13); otherwise close it with a test."
      fi
      ;;
    esac
    # Entries next to each other share the comment above the first.
    previous=$text
  done < <(show "$mutants")
fi

if ((found > 0)); then
  echo "$found line(s) put off a fix or switch off a check without a reason." >&2
  exit 1
fi
