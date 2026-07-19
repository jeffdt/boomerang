#!/usr/bin/env bash
# Resets jeffdt/universe (the disposable sandbox repo used by
# docs/demo/*.tape recordings, see AGENTS.md's "Regenerating the README
# demo GIFs") to a known-good state before recording:
#
#   - the label set is reconciled to exactly DESIRED_LABELS below: anything
#     missing gets created, anything extra (leftover GitHub defaults, or a
#     label since renamed away from) gets deleted. Keeping this list short
#     and curated is what lets boomerang's Labels picker show every option
#     on screen at once - see AGENTS.md for the scroll-follow limitation
#     this sidesteps.
#   - the four FILLER_ISSUES exist with their intended title/labels/body,
#     reconciled the same way on every run (labels and body get overwritten
#     to match), so drift from a previous recording session can't accumulate.
#   - exactly one "$FEATURED_TITLE" issue survives across recordings, reset
#     to a pristine just-captured state every run (no body, no labels).
#     edit-issue.tape adds those live, so this issue needs to start empty
#     every time, not just the first. Duplicates left over from prior
#     recording iterations get closed.
#
# Every step is idempotent, so it's safe to run before every recording
# session regardless of what state the sandbox repo is currently in.
#
#   docs/demo/seed-issues.sh

set -euo pipefail

REPO="jeffdt/universe"
FEATURED_TITLE="Check if light speed is constant for every observer"

# name|color|description
DESIRED_LABELS=(
  "bug|d73a4a|Something isn't working"
  "docs|0075ca|Improvements or additions to documentation"
  "feature|a2eeef|New feature or request"
  "good first issue|7057ff|Good for newcomers"
  "spike|8B5CF6|Time-boxed investigation, not committed work"
)

# title|comma-separated labels|body
FILLER_ISSUES=(
  "Solve FTL travel|feature,good first issue|Should be simple enough. Add energy until you reach c, then add some more. 🥳 Should only touch a handful of files."
  "Add wormholes for local debugging|feature|Getting to another galaxy from the current one is currently O(n) hops. In production this is unavoidable, but for local dev, a deployable wormhole would make traversal much faster."
  "Opened box and cat was neither alive nor dead|bug|Expected: observation collapses the cat to alive or dead. Actual: state stays superposed indefinitely. Reproducible 100% of the time, which is itself suspicious for a quantum system."
  "Explain why light has a speed limit|docs|Docs indicate that light has a speed limit, but does not mention why. Add a short doc clarifying that c is the rate limit of causality itself, so nothing can exceed it, light included."
)

echo "==> Reconciling labels to the curated set"
current_labels="$(gh label list -R "$REPO" --json name -q '.[].name')"

for entry in "${DESIRED_LABELS[@]}"; do
  IFS='|' read -r name color description <<< "$entry"
  if grep -qxF "$name" <<< "$current_labels"; then
    gh label edit "$name" -R "$REPO" --color "$color" --description "$description" >/dev/null
  else
    echo "    creating: $name"
    gh label create "$name" -R "$REPO" --color "$color" --description "$description" >/dev/null
  fi
done

desired_names="$(printf '%s\n' "${DESIRED_LABELS[@]}" | cut -d'|' -f1)"
while IFS= read -r name; do
  [ -z "$name" ] && continue
  if ! grep -qxF "$name" <<< "$desired_names"; then
    echo "    deleting: $name"
    gh label delete "$name" -R "$REPO" --yes >/dev/null
  fi
done <<< "$current_labels"

echo "==> Reconciling filler issues"
existing_titles="$(gh issue list -R "$REPO" --state all --json title -q '.[].title')"
for entry in "${FILLER_ISSUES[@]}"; do
  IFS='|' read -r title labels body <<< "$entry"
  IFS=',' read -ra label_args <<< "$labels"
  label_flags=()
  for l in "${label_args[@]}"; do
    label_flags+=(--add-label "$l")
  done

  if grep -qxF "$title" <<< "$existing_titles"; then
    echo "    updating: $title"
    number="$(gh issue list -R "$REPO" --state all --json number,title -q ".[] | select(.title == \"$title\") | .number" | head -1)"
    gh issue edit "$number" -R "$REPO" --body "$body" "${label_flags[@]}" >/dev/null
  else
    echo "    creating: $title"
    gh issue create -R "$REPO" --title "$title" --body "$body" "${label_flags[@]}" >/dev/null
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

echo "==> Current state of $REPO"
gh issue list -R "$REPO" --state open --json number,title,labels \
  -q '.[] | "#\(.number)  \(.title)  \([.labels[].name] | join(","))"'
