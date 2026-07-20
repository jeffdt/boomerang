# Regenerates docs/images/browse-and-yank.gif for the README.
#
# Prerequisites:
#   - Run docs/demo/seed-issues.sh first so jeffdt/universe has a
#     predictable, deduplicated issue list (this tape assumes the "Check if
#     light speed is constant for every observer" spike issue is the
#     oldest issue, sorting last, with four filler issues above it in the
#     list - five open issues total).
#   - Run from the repo root: vhs docs/demo/browse-and-yank.tape
#   - `boomerang` on PATH (the real binary, not a dev build)
#   - `gh` authenticated
#   - jeffdt/universe cloned at ~/code/universe
#
# Like quick-capture.tape, this nests a real, isolated tmux server (its own
# socket, never the default one) so the actual `display-popup` chrome
# renders - this recording's whole point is showing the popup interrupting
# a real shell, not just boomerang's own UI in isolation. docs/demo/
# demo.tmux.conf's `i` bind (added alongside the existing `I` capture bind)
# mirrors the real `bind i ... "exec boomerang"` from the README. The pane
# runs `zsh -f` (no rc files) so a themed prompt doesn't fight `Set Theme`,
# and XDG_CONFIG_HOME points at a scratch directory (with a symlinked-in
# `gh/` preserving real auth) before the nested tmux server even starts, so
# the recording reflects boomerang's default settings rather than whatever
# is currently in Jeff's real config.toml.
#
# Story: the recording opens on a bare, already-waiting shell prompt - nothing
# happened yet, this *is* the start of the task, unlike quick-capture's
# mid-flow interruption. The full command line is typed out, closing quote
# and all: `claude -p "let's implement these issues as a bundle: "` - the way
# a shell with auto-paired quotes would leave it, or just how someone drafts
# a quoted string in one go. Typed in a few chunks at varying TypingSpeed
# (see the vhs-tmux-recording skill's "vary typing speed" step) rather than
# one uniform speed for the whole line, since a constant per-character delay
# reads as obviously scripted the moment there's a full sentence on screen.
# The cursor then steps left, back inside the
# quotes, before the popup opens - this is what makes the eventual paste land
# in the middle of the line instead of magically appearing after a quote that
# wasn't there yet. The popup (Ctrl+B i) opens over that half-typed command
# and browses down through the list first - not a straight line to any
# target, as if genuinely scanning what's open - then works back upward,
# checking off three issues along the way (Space) as each one turns out to
# be relevant, rather than deciding on all three up front. `y` yanks every
# checked issue as one comma-joined `#7, #9, #12`, closing lands control
# right back in the still-half-typed shell command with the cursor still
# parked before the closing quote, and pasting drops the numbers straight in.
# This is the multi-select follow-up to the single-issue paste story that
# used to live here.

Output "docs/images/browse-and-yank.gif"

Set Shell zsh
Set FontSize 14
Set Width 1100
Set Height 950
Set Theme "GruvboxDark"

Hide
Type "unset TMUX; export XDG_CONFIG_HOME=$(mktemp -d); mkdir -p $XDG_CONFIG_HOME/gh; ln -s ~/.config/gh/*.yml $XDG_CONFIG_HOME/gh/; BOOMERANG_REPO=$PWD; cd ~/code/universe && tmux -L boomerang-demo-gif -f $BOOMERANG_REPO/docs/demo/demo.tmux.conf new-session -s demo zsh -f"
Enter
Sleep 2s
Show

Sleep 600ms
Set TypingSpeed 30ms
Type `claude -p "`
Set TypingSpeed 60ms
Type `let's implement `
Sleep 150ms
Set TypingSpeed 35ms
Type `these issues `
Set TypingSpeed 55ms
Type `as a bundle: "`
Set TypingSpeed 50ms
Sleep 500ms
Left
Sleep 900ms

Ctrl+B
Type "i"
Sleep 2000ms

Down
Sleep 350ms
Down
Sleep 350ms
Down
Sleep 700ms
Space
Sleep 700ms
Up
Sleep 700ms
Space
Sleep 700ms
Up
Sleep 350ms
Up
Sleep 700ms
Space
Sleep 900ms
Type "y"
Sleep 900ms
Type "q"
Sleep 900ms

Paste
Sleep 2s

Hide
Ctrl+C
Sleep 200ms
Type "exit"
Enter
Sleep 500ms
