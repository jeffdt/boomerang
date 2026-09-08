# Regenerates docs/images/settings.gif for the README's Settings section.
#
# Prerequisites:
#   - Run docs/demo/seed-issues.sh first, so the list behind Settings shows
#     the same predictable five-issue state the other tapes assume.
#   - Run from the repo root: vhs docs/demo/settings.tape
#   - `boomerang` on PATH (the real binary, not a dev build)
#   - `gh` authenticated
#   - jeffdt/universe cloned at ~/code/universe
#
# Like edit-issue.tape, this runs boomerang directly in the recorded shell
# rather than nesting an isolated tmux server - Settings is boomerang's own
# full-screen view, nothing about `display-popup` chrome belongs on screen
# here. Still isolates XDG_CONFIG_HOME (preserving gh's real auth via a
# symlinked-in gh/ config dir) so the recording starts from boomerang's
# default settings rather than whatever's currently in Jeff's real
# config.toml - the whole point of this recording is to *change* those
# settings on screen, so it has to start from a known baseline. Still
# launches `zsh -f` so a themed prompt doesn't fight `Set Theme`, and
# `clear`s right before boomerang launches so the hidden setup command
# doesn't reappear in the primary screen buffer once boomerang quits.
#
# Story: `,` opens Settings. Zebra striping toggles off first (Space) -
# picked because its effect is immediately checkable once back on the
# list, not just a label change inside the settings screen itself. Then
# Accent color and Repo color each cycle forward one step (Blue -> Magenta,
# Green -> Yellow) - two colors chosen for maximum on-screen contrast with
# each other and with the GruvboxDark backdrop, not the colors someone
# would necessarily pick for daily use. `Esc` returns to the list so the
# combined effect - flat rows, a magenta-accented frame, a yellow repo
# name - is the payoff frame, proving these aren't just labels changing in
# a settings menu but the picker's actual on-screen look.

Output "docs/images/settings.gif"

Set Shell zsh
Set FontSize 14
Set Width 1100
Set Height 950
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

Type ","
Sleep 900ms

Down
Sleep 400ms
Space
Sleep 700ms

Down@200ms 2
Sleep 400ms
Type "l"
Sleep 700ms

Down
Sleep 400ms
Type "l"
Sleep 900ms

Escape
Sleep 2500ms

Hide
Type "q"
Sleep 300ms
