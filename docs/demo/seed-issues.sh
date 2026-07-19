#!/usr/bin/env bash
# Resets jeffdt/universe (the disposable sandbox repo used by
# docs/demo/*.tape recordings, see AGENTS.md's "Regenerating the README
# demo GIFs") to a known-good state before recording:
#
#   - exactly one "spike: check if light speed is constant" issue survives
#     across recordings, reset to a pristine just-captured state every time
#     (no body, no labels). The edit-issue demo adds those live, so this
#     issue needs to start empty on every run, not just the first one;
#     duplicates left over from prior recording iterations get closed. The
#     "spike" label itself is created (if missing) so it's selectable in
#     the edit form, without being pre-applied to the issue.
#   - a couple of the least-relevant default labels (invalid, wontfix) get
#     removed so boomerang's label picker has few enough entries that
#     "spike" (the newest, so it sorts last) still lands inside the
#     picker's visible window. The picker doesn't scroll to follow the
#     cursor past what fits on screen (src/ui.rs render_labels renders a
#     plain ratatui List with no ListState/scroll-offset - worth fixing in
#     boomerang itself at some point), so this keeps the edit-issue demo
#     honest without relying on that fix.
#   - a small curated set of filler issues exists so the picker/multi-select
#     demos show a real, varied-looking list instead of five copies of the
#     same title.
#
# Every step is idempotent, so it's safe to run before every recording
# session regardless of what state the sandbox repo is currently in.
#
#   docs/demo/seed-issues.sh

set -euo pipefail

REPO="jeffdt/universe"
FEATURED_TITLE="spike: check if light speed is constant for every observer"
SPIKE_LABEL="spike"
LABELS_TO_TRIM=(invalid wontfix)

FILLER_TITLES=(
  "bug: apple falls up when observed on Tuesdays"
  "fix: entropy occasionally decreases in prod"
  "chore: recalibrate flux capacitor before next deploy"
  "feat: add support for wormhole-based caching"
  "docs: explain why the cat is both alive and dead in the README"
)

echo "==> Ensuring '$SPIKE_LABEL' label exists"
if ! gh label list -R "$REPO" --json name -q '.[].name' | grep -qx "$SPIKE_LABEL"; then
  gh label create "$SPIKE_LABEL" -R "$REPO" --color "8B5CF6" \
    --description "Time-boxed investigation, not committed work"
fi

echo "==> Trimming least-relevant default labels"
for l in "${LABELS_TO_TRIM[@]}"; do
  if gh label list -R "$REPO" --json name -q '.[].name' | grep -qx "$l"; then
    echo "    deleting: $l"
    gh label delete "$l" -R "$REPO" --yes >/dev/null
  fi
done

echo "==> Deduplicating '$FEATURED_TITLE'"
featured_numbers=()
while IFS= read -r n; do
  [ -n "$n" ] && featured_numbers+=("$n")
done < <(
  gh issue list -R "$REPO" --state open --json number,title \
    -q ".[] | select(.title == \"$FEATURED_TITLE\") | .number" | sort -n
)

if [ "${#featured_numbers[@]}" -eq 0 ]; then
  echo "    none open, creating one"
  keep="$(gh issue create -R "$REPO" --title "$FEATURED_TITLE" --body "" | grep -oE '[0-9]+$')"
else
  keep="${featured_numbers[0]}"
  echo "    keeping #$keep"
  for n in "${featured_numbers[@]:1}"; do
    echo "    closing duplicate #$n"
    gh issue close "$n" -R "$REPO" -c "Closing - duplicate created while recording a boomerang demo." >/dev/null
  done
fi

echo "==> Resetting #$keep to a pristine just-captured state (no body, no labels)"
gh issue edit "$keep" -R "$REPO" --body "" >/dev/null
current_labels="$(gh issue view "$keep" -R "$REPO" --json labels -q '.labels[].name')"
if [ -n "$current_labels" ]; then
  while IFS= read -r l; do
    gh issue edit "$keep" -R "$REPO" --remove-label "$l" >/dev/null
  done <<< "$current_labels"
fi

echo "==> Ensuring filler issues exist"
existing_titles="$(gh issue list -R "$REPO" --state all --json title -q '.[].title')"
for title in "${FILLER_TITLES[@]}"; do
  if grep -qxF "$title" <<< "$existing_titles"; then
    echo "    already exists: $title"
  else
    echo "    creating: $title"
    gh issue create -R "$REPO" --title "$title" --body "" >/dev/null
  fi
done

echo "==> Current state of $REPO"
gh issue list -R "$REPO" --state open --json number,title,labels \
  -q '.[] | "#\(.number)  \(.title)  \([.labels[].name] | join(","))"'
