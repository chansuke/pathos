#!/usr/bin/env bash
# Prints the items of docs/verification.md whose trigger matches a change.
#
#   verification-checklist.sh [BASE]     BASE (default: origin/main) to the
#                                        working tree, untracked files included
#   verification-checklist.sh BASE HEAD  BASE to the commit HEAD
set -euo pipefail

base=${1:-origin/main}
root=$(git rev-parse --show-toplevel)
checklist="$root/docs/verification.md"

if (($# > 1)); then
    revisions=("$base" "$2")
    untracked=()
else
    revisions=("$base")
    mapfile -t untracked < <(git -C "$root" ls-files --others --exclude-standard)
fi
mapfile -t changed < <(git -C "$root" diff --name-only "${revisions[@]}" --)
changed+=("${untracked[@]}")
added=$(git -C "$root" diff -U0 "${revisions[@]}" -- | grep '^+' | grep -v '^+++' || true)

# The text between backticks on one line, one per output line.
quoted() {
    # shellcheck disable=SC2016 # the backticks are literal
    grep -o '`[^`]*`' <<<"$1" | sed 's/^`//; s/`$//'
}

block=""
matched=false
found=0
flush() {
    if $matched; then
        printf '%s\n' "$block"
        found=$((found + 1))
    fi
    block=""
    matched=false
}

in_items=false
while IFS= read -r line; do
    if [[ $line == '## Items' ]]; then
        in_items=true
        continue
    fi
    $in_items || continue
    if [[ $line == '### '* ]]; then
        flush
    fi
    block+="$line"$'\n'
    case $line in
    '- Paths: '*)
        while IFS= read -r pattern; do
            for file in "${changed[@]}"; do
                # shellcheck disable=SC2053 # the pattern is meant to glob
                if [[ $file == $pattern ]]; then
                    matched=true
                fi
            done
        done < <(quoted "$line")
        ;;
    '- Added lines: '*)
        while IFS= read -r pattern; do
            if [[ -n $added ]] && grep -Eq -- "$pattern" <<<"$added"; then
                matched=true
            fi
        done < <(quoted "$line")
        ;;
    esac
done <"$checklist"
flush

if ((found == 0)); then
    echo "No checklist item matches this change."
fi
