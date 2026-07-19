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
# Unlike quick-capture.tape, this one doesn't need the isolated nested tmux
# server: nothing about `display-popup` chrome is on screen here, it's all
# boomerang's own list view, so it runs directly in the recorded shell.
# Still launches `zsh -f` (no rc files) so a themed prompt doesn't fight
# `Set Theme` during the visible shell moments before/after boomerang runs,
# and points XDG_CONFIG_HOME at a scratch directory so the recording always
# starts from boomerang's default settings rather than whatever's currently
# in Jeff's real config.toml (exit-on-copy in particular changes the whole
# shape of this recording if it's on). `gh` also honors XDG_CONFIG_HOME, so
# the scratch dir gets a symlinked-in `gh/` pointing at the real config —
# otherwise `gh` loses its auth and boomerang can't detect the repo at all.
#
# Story: check two filler issues and yank them together (multi-select),
# then yank the "spike" issue on its own and paste the bare issue number
# straight into a `claude -p` invocation - the payoff of "catch it again
# when it's time to build it."
#
# The hidden setup ends with `clear` right before boomerang launches: since
# boomerang is a fullscreen (alt-screen) TUI, quitting it restores whatever
# was on the primary screen buffer *before* it launched - including any
# "hidden" typing, since `Hide` only skips rendering those frames, it
# doesn't erase the pty's actual scrollback. Without the `clear`, the whole
# setup command reappears the moment boomerang exits back to the shell.

Output "docs/images/browse-and-yank.gif"

Set Shell zsh
Set FontSize 14
Set Width 1100
Set Height 700
Set Theme "GruvboxDark"

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

Space
Sleep 400ms
Down
Sleep 300ms
Space
Sleep 500ms
Type "y"
Sleep 1500ms

Down
Sleep 200ms
Down
Sleep 200ms
Down
Sleep 400ms
Type "y"
Sleep 1500ms

Type "q"
Sleep 500ms

Type `claude -p "help me tackle issue `
Paste
Type `"`
Sleep 2s
