#!/usr/bin/env bash
# Tests scripts/reaper-key, scripts/check-reaper-line.sh and the line the
# pre-push hook prints, against a throwaway repository and without REAPER.
set -euo pipefail

source_root=$(cd "$(dirname "$0")/.." && pwd)
repo=$(mktemp -d)
trap 'rm -rf "$repo"' EXIT
cd "$repo"
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
commit() { git -c user.name=t -c user.email=t@example.com commit -q "$@"; }
git init -q -b main
mkdir -p scripts .githooks crates/pathos-core crates/pathos-render crates/pathos-reaper src
cp "$source_root/scripts/reaper-key" "$source_root/scripts/check-reaper-line.sh" scripts/
cp "$source_root/.githooks/pre-push" .githooks/
for path in crates/pathos-core crates/pathos-render crates/pathos-reaper src; do
  echo one >"$path/file"
done
git add -A
commit -m base
base=$(git rev-parse HEAD)
failures=0
fail() {
  echo "FAIL: $1"
  failures=1
}

# expect WANT(pass|fail) DESCRIPTION HEAD BODY
expect() {
  local status=0
  BODY=$4 scripts/check-reaper-line.sh "$base" "$3" >/dev/null 2>&1 || status=$?
  if { [ "$1" = pass ] && [ "$status" -ne 0 ]; } || { [ "$1" = fail ] && [ "$status" -ne 1 ]; }; then
    fail "$2 (exit $status)"
  fi
}

echo two >src/file
git add -A
commit -m app
app=$(git rev-parse HEAD)
[ "$(scripts/reaper-key "$app")" = "$(scripts/reaper-key "$base")" ] ||
  fail "a change outside the compared paths changes the key"
expect pass "a change outside the compared paths needs no line" "$app" ""

echo two >crates/pathos-render/file
git add -A
commit -m render
render=$(git rev-parse HEAD)
key=$(scripts/reaper-key "$render")
[ "$key" != "$(scripts/reaper-key "$app")" ] || fail "a change to the render keeps the key"
expect fail "no line" "$render" "Some change."
expect fail "a line without a key" "$render" $'Some change.\n\nREAPER comparison: passed'
expect fail "a line with the key of other code" "$render" "REAPER comparison: passed $(scripts/reaper-key "$app")"
expect pass "a line with the key of the head" "$render" $'Some change.\r\n\r\nREAPER comparison: passed '"$key"$'\r'

# Missing origin/main (merge-base failure) fails clearly rather than skipping.
missing_status=0
missing_out=$(echo "refs/heads/main $render refs/heads/main 0000000000000000000000000000000000000000" |
  .githooks/pre-push 2>&1) || missing_status=$?
if [ "$missing_status" -eq 0 ]; then
  fail "the hook silently skipped when origin/main was missing"
elif ! grep -q "merge-base" <<<"$missing_out"; then
  fail "the hook did not explain the merge-base failure: $missing_out"
fi

# The hook prints the line for code that passed before without running
# REAPER again.
git update-ref refs/remotes/origin/main "$base"
trees=$(for path in crates/pathos-core crates/pathos-render crates/pathos-reaper; do
  git rev-parse "$render:$path"
done | tr '\n' ' ')
echo "$trees" >"$(git rev-parse --git-path pathos-reaper-passed)"
printed=$(echo "refs/heads/main $render refs/heads/main 0000000000000000000000000000000000000000" |
  .githooks/pre-push 2>&1) || fail "the hook failed on code that passed before: $printed"
grep -qx "REAPER comparison: passed $key" <<<"$printed" ||
  fail "the hook prints another line than the workflow expects: $printed"

if [ "$failures" -ne 0 ]; then
  exit 1
fi
echo "test-reaper-line.sh: all cases pass"
