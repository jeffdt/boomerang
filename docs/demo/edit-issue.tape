# Regenerates docs/images/edit-issue.gif for the README.
#
# Prerequisites:
#   - Run docs/demo/seed-issues.sh first. It resets the "spike: check if
#     light speed is constant" issue to a pristine just-captured state (no
#     body, no labels) on every run, which this tape depends on - it's the
#     one adding those live.
#   - Run from the repo root: vhs docs/demo/edit-issue.tape
#   - `boomerang` on PATH (the real binary, not a dev build)
#   - `gh` authenticated
#   - jeffdt/universe cloned at ~/code/universe
#
# Like browse-and-yank.tape, this runs boomerang directly in the recorded
# shell rather than nesting an isolated tmux server - nothing about
# `display-popup` chrome is on screen here. Still isolates XDG_CONFIG_HOME
# (preserving gh's real auth via a symlinked-in gh/ config dir) so the
# recording doesn't depend on whatever's in Jeff's real config.toml, and
# still launches `zsh -f` so a themed prompt doesn't fight `Set Theme`.
# `clear` right before boomerang launches keeps that hidden setup out of
# the primary screen buffer boomerang restores on quit.
#
# Story: this is the sequel to quick-capture.gif. That one shows an idea
# caught on a whim with nothing but a title; this one shows it fleshed out
# later - body written, "spike" label attached - once there's actually
# time to think it through. Search finds it by title rather than scrolling,
# since "where did I put that idea" is the whole reason capture-now,
# flesh-out-later works: you don't have to remember where you left it.
#
# The Labels field renders as a plain list with no scroll-follow (see
# src/ui.rs render_labels - a ratatui List with no ListState, so the
# visible window never moves even if the cursor does; worth fixing in
# boomerang itself at some point, but not attempted here). seed-issues.sh
# works around it by trimming the repo's default labels down to 8, so
# "spike" (the newest, sorting last) still lands inside the visible
# window - the 7 Down presses below land exactly on it.

Output "docs/images/edit-issue.gif"

Set Shell zsh
Set FontSize 14
Set Width 1100
Set Height 700
Set Theme "GruvboxDark"
Set TypingSpeed 25ms

Hide
Type "exec zsh -f"
Enter
Sleep 300ms
Type "export XDG_CONFIG_HOME=$(mktemp -d); mkdir -p $XDG_CONFIG_HOME/gh; ln -s ~/.config/gh/*.yml $XDG_CONFIG_HOME/gh/; cd ~/code/universe"
Enter
Sleep 300ms
Type "clear"
Enter
Sleep 200ms
Show

Type "boomerang"
Enter
Sleep 1800ms

Type "/"
Sleep 300ms
Type "light speed"
Sleep 700ms
Enter
Sleep 700ms

Type "e"
Sleep 900ms

Tab
Sleep 400ms
Type "Special relativity confirms it: c is invariant across inertial frames (Einstein, 1905). Traced it through the Michelson-Morley result by hand - checks out."
Sleep 600ms
Tab
Sleep 500ms
Down@150ms 7
Sleep 400ms
Space
Sleep 600ms
Tab
Sleep 400ms
Enter
Sleep 2s

Hide
Type "q"
Sleep 300ms
