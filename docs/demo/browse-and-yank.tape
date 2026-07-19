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
# mid-flow interruption. Typing begins on screen: `claude -p "help me tackle
# issue `, then a pause, as if realizing the issue number isn't memorized.
# The popup (Ctrl+B i) opens over the half-typed command, browses down/up/
# down as if genuinely weighing a couple of issues before settling on one -
# not a straight line to the target - then yanks it and closes. Control
# lands right back in the still-half-typed shell command, and pasting the
# yanked number finishes it. No multi-select in this one; that's its own
# future recording rather than force-fit here.

Output "docs/images/browse-and-yank.gif"

Set Shell zsh
Set FontSize 14
Set Width 1100
Set Height 700
Set Theme "GruvboxDark"

Hide
Type "unset TMUX; export XDG_CONFIG_HOME=$(mktemp -d); mkdir -p $XDG_CONFIG_HOME/gh; ln -s ~/.config/gh/*.yml $XDG_CONFIG_HOME/gh/; BOOMERANG_REPO=$PWD; cd ~/code/universe && tmux -L boomerang-demo-gif -f $BOOMERANG_REPO/docs/demo/demo.tmux.conf new-session -s demo zsh -f"
Enter
Sleep 2s
Show

Sleep 600ms
Type `claude -p "help me tackle issue `
Sleep 900ms

Ctrl+B
Type "i"
Sleep 1400ms

Down
Sleep 250ms
Down
Sleep 250ms
Down
Sleep 500ms
Up
Sleep 250ms
Up
Sleep 500ms
Down
Sleep 250ms
Down
Sleep 250ms
Down
Sleep 400ms
Type "y"
Sleep 800ms
Type "q"
Sleep 900ms

Paste
Type `"`
Sleep 2s

Hide
Ctrl+C
Sleep 200ms
Type "exit"
Enter
Sleep 500ms
