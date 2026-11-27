#!/usr/bin/env bash
# Tests check-escape-hatches.sh against a throwaway repository.
set -euo pipefail

check=$(cd "$(dirname "$0")" && pwd)/check-escape-hatches.sh
repo=$(mktemp -d)
trap 'rm -rf "$repo"' EXIT
cd "$repo"
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
commit() { git -c user.name=t -c user.email=t@example.com commit -q "$@"; }
git init -q -b main
mkdir -p .cargo crates/core/tests .github/workflows
printf 'const MIN_HITS: u32 = 100;\nconst COVERAGE_TRACES: u32 = 2000;\n' >crates/core/tests/differential.rs
printf 'env:\n  PROPTEST_CASES: 10000\n' >.github/workflows/spec.yml
printf 'cc 1111\ncc 2222\n' >crates/core/tests/differential.proptest-regressions
printf 'exclude_re = [\n    # No test can see it.\n    "replace < with <=",\n]\n' >.cargo/mutants.toml
git add -A
commit -m base
failures=0

# expect WANT(pass|fail) DESCRIPTION FILE CONTENT: stages FILE with CONTENT,
# runs the check on the index, and restores FILE.
expect() {
  local want=$1 what=$2 status=0
  mkdir -p "$(dirname "$3")"
  printf '%s\n' "$4" >"$3"
  git add "$3"
  "$check" --staged >/dev/null 2>&1 || status=$?
  if { [ "$want" = pass ] && [ "$status" -ne 0 ]; } || { [ "$want" = fail ] && [ "$status" -ne 1 ]; }; then
    echo "FAIL: $what (exit $status)"
    failures=1
  fi
  if git cat-file -e "HEAD:$3" 2>/dev/null; then
    git checkout -q HEAD -- "$3"
  else
    git rm -q --cached "$3"
    rm -f "$3"
  fi
}

expect pass "plain code" src/a.rs 'fn main() {}'
expect fail "a TODO comment" src/a.rs 'fn main() {} // TODO: handle errors'
expect fail "a FIXME in Lean" spec/Pathos/A.lean 'theorem t : True := trivial -- FIXME'
expect fail "a workaround comment" src/a.rs '// Workaround for the flaky test'
expect fail "for now in a workflow" .github/workflows/x.yml 'run: true # for now'
expect pass "a word in a string, not a comment" src/a.rs 'let s = "TODO list";'
expect pass "an attribute is not a comment" src/a.rs '#[cfg(test)]'
expect fail "an ignored test without a reason" src/a.rs '#[ignore]'
expect fail "an ignored test with an empty reason" src/a.rs '#[ignore = ""]'
expect fail "an ignored test with a blank reason" src/a.rs '#[ignore = "   "]'
expect pass "an ignored test with a reason" src/a.rs '#[ignore = "runs REAPER"]'
expect fail "a TODO in a Rust block comment" src/a.rs 'fn main() { /* TODO: fix */ }'
expect fail "a workaround in a multi-line Rust block comment" src/a.rs $'/*\n * Workaround for bug\n */'
expect pass "a block comment marker inside a Rust string" src/a.rs 'let s = "/* TODO */";'
expect fail "a shellcheck directive without a reason" .githooks/pre-push '# shellcheck disable=SC2086'
expect pass "a shellcheck directive with a reason" run.sh '# shellcheck disable=SC2086 # split on purpose'
expect pass "prose in docs" docs/usage.md 'For now, only Linux is supported. TODO list'
expect fail "a lower minimum" crates/core/tests/differential.rs $'const MIN_HITS: u32 = 50;\nconst COVERAGE_TRACES: u32 = 2000;'
expect pass "a higher minimum" crates/core/tests/differential.rs $'const MIN_HITS: u32 = 200;\nconst COVERAGE_TRACES: u32 = 2000;'
expect fail "fewer traces" crates/core/tests/differential.rs $'const MIN_HITS: u32 = 100;\nconst COVERAGE_TRACES: u32 = 1_000;'
expect fail "fewer cases in CI" .github/workflows/spec.yml $'env:\n  PROPTEST_CASES: 100'
expect fail "deleted PROPTEST_CASES in CI" .github/workflows/spec.yml 'env:'
expect fail "cross-hunk lowering of a threshold" crates/core/tests/differential.rs $'const COVERAGE_TRACES: u32 = 2000;\nfn pad1() {}\nfn pad2() {}\nfn pad3() {}\nfn pad4() {}\nconst MIN_HITS: u32 = 50;'
expect pass "cross-hunk move of a threshold without lowering" crates/core/tests/differential.rs $'const COVERAGE_TRACES: u32 = 2000;\nfn pad1() {}\nfn pad2() {}\nfn pad3() {}\nfn pad4() {}\nconst MIN_HITS: u32 = 100;'
expect fail "a removed seed" crates/core/tests/differential.proptest-regressions 'cc 1111'
expect pass "an added seed" crates/core/tests/differential.proptest-regressions $'cc 1111\ncc 2222\ncc 3333'
expect fail "an exclusion without a reason" .cargo/mutants.toml $'exclude_re = [\n    # No test can see it.\n    "replace < with <=",\n\n    "replace + with -",\n]'
expect fail "a single-line exclusion list without a reason" .cargo/mutants.toml 'exclude_re = ["replace + with -"]'
expect pass "a single-line exclusion list with a reason" .cargo/mutants.toml $'# No test can see it.\nexclude_re = ["replace + with -"]'
expect pass "an exclusion with a reason" .cargo/mutants.toml $'exclude_re = [\n    # No test can see it.\n    "replace < with <=",\n    # Nor this one.\n    "replace + with -",\n]'

# A range reports the line number in the new file.
printf 'a\nb\n' >f.rs
git add f.rs
commit -m two
printf 'a\nb\n// TODO c\n' >f.rs
git add f.rs
commit -m three
reported=$("$check" HEAD~1 HEAD 2>&1 || true)
if ! grep -q '^f.rs:3: // TODO c$' <<<"$reported"; then
  echo "FAIL: the line number of an added line"
  failures=1
fi

if [ "$failures" -ne 0 ]; then
  exit 1
fi
echo "check-escape-hatches.sh: all cases pass"
