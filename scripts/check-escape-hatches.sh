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
  $'code\t*.rs\t#\\[[[:space:]]*ignore([[:space:]]*=[[:space:]]*"[[:space:]]*")?[[:space:]]*\\]\tGive the reason: #[ignore = "why it does not run by default"], as the REAPER tests do. A test that fails is fixed, not ignored.'
  $'code\t*.sh\t#[[:space:]]*shellcheck[[:space:]]+disable=[A-Z0-9,]+[[:space:]]*$\tSay why after the directive: `# shellcheck disable=SC2086 # the paths are split on purpose`.'
  $'code\t.githooks/*\t#[[:space:]]*shellcheck[[:space:]]+disable=[A-Z0-9,]+[[:space:]]*$\tSay why after the directive: `# shellcheck disable=SC2086 # the paths are split on purpose`.'
)

# Extracts the comment portion of a Rust line (both `//` and `/* ... */`),
# updating `in_block_comment` across lines in the hunk and setting `comment`.
in_block_comment=false
rs_comment() { # LINE
  local rest=$1
  comment=""
  while [ -n "$rest" ]; do
    if $in_block_comment; then
      if [[ $rest == *'*/'* ]]; then
        comment+=" ${rest%%'*/'*}*/"
        rest=${rest#*'*/'}
        in_block_comment=false
      else
        comment+=" $rest"
        break
      fi
    else
      local i=0 len=${#rest} in_str=false esc=false ch next found_kind=""
      while ((i < len)); do
        ch=${rest:i:1}
        if $in_str; then
          if $esc; then
            esc=false
          elif [ "$ch" = '\' ]; then
            esc=true
          elif [ "$ch" = '"' ]; then
            in_str=false
          fi
        else
          if [ "$ch" = "'" ] && [ "${rest:i+1:2}" = "\"'" ]; then
            i=$((i + 3))
            continue
          elif [ "$ch" = '"' ]; then
            in_str=true
          elif [ "$ch" = '/' ] && ((i + 1 < len)); then
            next=${rest:i+1:1}
            if [ "$next" = '/' ]; then
              comment+=" ${rest:i}"
              rest=""
              found_kind=line
              break
            elif [ "$next" = '*' ]; then
              rest=${rest:i}
              in_block_comment=true
              found_kind=block
              break
            fi
          fi
        fi
        i=$((i + 1))
      done
      [ -z "$found_kind" ] && break
      if [ "$found_kind" = block ]; then
        local part=${rest:2}
        if [[ $part == *'*/'* ]]; then
          comment+=" /*${part%%'*/'*}*/"
          rest=${part#*'*/'}
          in_block_comment=false
        else
          comment+=" $rest"
          break
        fi
      fi
    fi
  done
}

# The comment part of LINE in FILE, stored in `comment`, empty when it has none.
# Rust uses `//` and `/* ... */`, Lean `--`, and the others `#`; a `#` that
# starts an attribute such as `#[cfg]` is code.
comment_of() { # FILE LINE
  comment=""
  case $1 in
  *.rs) rs_comment "$2" ;;
  *.lean) [[ $2 == *--* ]] && comment="--${2#*--}" ;;
  *.sh | *.toml | *.yaml | *.yml | *.nix | *.tla | *.cfg | .githooks/*)
    [[ $2 =~ (^|[[:space:]])#([^[!]|$) ]] && comment="#${2#*#}"
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

threshold_file() { # FILE
  checked "$1" || return 1
  case $1 in
  */tests/* | .github/workflows/*) return 0 ;;
  *) return 1 ;;
  esac
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
threshold_advice='Do not lower or remove a minimum or a number of cases to make a test pass. Make the generators reach the rule more often, or fix the code; the minimums are what ADR 4 and ADR 13 rely on.'

found=0
report() { # FILE LINE TEXT ADVICE
  printf '%s:%s: %s\n    %s\n' "$1" "$2" "${3#"${3%%[![:space:]]*}"}" "$4" >&2
  found=$((found + 1))
}

# Track thresholds across the whole file so removing a threshold or moving it
# to another hunk with a lower value cannot slip past the check.
declare -A removed_max=() removed_line=() removed_text=()
declare -A added_min=() added_line=() added_text=()
flush_thresholds() {
  if [ -n "$file" ] && threshold_file "$file"; then
    for pattern in "${thresholds[@]}"; do
      [ -n "${removed_max[$pattern]:-}" ] || continue
      if [ -z "${added_min[$pattern]:-}" ]; then
        report "$file" "${removed_line[$pattern]}" "${removed_text[$pattern]}" "$threshold_advice"
      elif ((added_min[$pattern] < removed_max[$pattern])); then
        report "$file" "${added_line[$pattern]}" "${added_text[$pattern]}" "$threshold_advice"
      fi
    done
  fi
  removed_max=()
  removed_line=()
  removed_text=()
  added_min=()
  added_line=()
  added_text=()
}

old_file=""
file=""
line=0
while IFS= read -r row; do
  case $row in
  '--- a/'*)
    old_file=${row#--- a/}
    continue
    ;;
  '+++ b/'*)
    flush_thresholds
    file=${row#+++ b/}
    in_block_comment=false
    continue
    ;;
  '+++ /dev/null')
    flush_thresholds
    file=$old_file
    in_block_comment=false
    continue
    ;;
  '+++ '* | '--- '*) continue ;;
  '@@ '*)
    # @@ -a,b +c,d @@: the next added line is line c of the new file.
    line=${row#*+}
    line=${line%%[, ]*}
    in_block_comment=false
    continue
    ;;
  '-'*)
    text=${row#-}
    if [[ $file == *.proptest-regressions && $text == cc\ * ]]; then
      report "$file" "$line" "$text" "Keep each seed that once failed, so the case runs again on every test run."
    fi
    if threshold_file "$file"; then
      for pattern in "${thresholds[@]}"; do
        if [[ $text =~ $pattern ]]; then
          val=${BASH_REMATCH[1]//_/}
          if [ -z "${removed_max[$pattern]:-}" ] || ((val > removed_max[$pattern])); then
            removed_max[$pattern]=$val
            removed_line[$pattern]=$line
            removed_text[$pattern]=$text
          fi
        fi
      done
    fi
    continue
    ;;
  '+'*) ;;
  *) continue ;;
  esac
  text=${row#+}
  if checked "$file"; then
    comment_of "$file" "$text"
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
    if threshold_file "$file"; then
      for pattern in "${thresholds[@]}"; do
        if [[ $text =~ $pattern ]]; then
          val=${BASH_REMATCH[1]//_/}
          if [ -z "${added_min[$pattern]:-}" ] || ((val < added_min[$pattern])); then
            added_min[$pattern]=$val
            added_line[$pattern]=$line
            added_text[$pattern]=$text
          fi
        fi
      done
    fi
  fi
  line=$((line + 1))
done < <("${diff[@]}")
flush_thresholds

# Every exclusion of cargo-mutants has its reason in the comment above it
# (ADR 13). The whole file is read, since an entry may move.
mutants=.cargo/mutants.toml
if "${diff[@]}" "$mutants" | grep -q '^+'; then
  number=0
  previous=""
  in_list=false
  while IFS= read -r text; do
    number=$((number + 1))
    if [[ $text =~ ^[[:space:]]*# ]]; then
      previous=$text
      continue
    fi
    starts_list=false
    if [[ $text == *'= ['* ]]; then
      in_list=true
      starts_list=true
    fi
    if $in_list && [[ $text == *\"* || $text == *\'* ]]; then
      if $starts_list; then
        if [[ ! $previous =~ ^[[:space:]]*# ]]; then
          report "$mutants" "$number" "$text" "Say in a comment above the entry why no test can catch this mutant (ADR 13); otherwise close it with a test."
        fi
      elif [[ $previous != *\"* && $previous != *\'* ]] && [[ ! $previous =~ ^[[:space:]]*# ]]; then
        report "$mutants" "$number" "$text" "Say in a comment above the entry why no test can catch this mutant (ADR 13); otherwise close it with a test."
      fi
    fi
    if $in_list && [[ $text == *']'* ]]; then
      in_list=false
    fi
    # Entries next to each other share the comment above the first.
    previous=$text
  done < <(show "$mutants")
fi

if ((found > 0)); then
  echo "$found line(s) put off a fix or switch off a check without a reason." >&2
  exit 1
fi
