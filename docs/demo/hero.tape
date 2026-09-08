# Regenerates docs/images/hero.gif, the README's lead demo.
#
# Prerequisites:
#   - Run docs/demo/seed-issues.sh first so jeffdt/universe has a
#     predictable, deduplicated issue list (this tape assumes the "Check if
#     light speed is constant for every observer" issue is the oldest
#     issue, sorting last with no labels yet, with four labeled filler
#     issues above it in the list - five open issues total).
#   - Run from the repo root: vhs docs/demo/hero.tape
#   - `boomerang` on PATH (the real binary, not a dev build)
#   - `gh` authenticated
#   - jeffdt/universe cloned at ~/code/universe
#
# Like quick-capture.tape, this nests a real, isolated tmux server (its own
# socket, never the default one) so the actual `display-popup` chrome
# renders - the whole point is showing the popup interrupting real work,
# not just boomerang's own UI in isolation. docs/demo/demo.tmux.conf's `i`
# bind mirrors the real `bind i ... "exec boomerang"` from the README. The
# pane runs `zsh -f` (no rc files) so a themed prompt doesn't fight `Set
# Theme`, and XDG_CONFIG_HOME points at a scratch directory (with a
# symlinked-in `gh/` preserving real auth) before the nested tmux server
# even starts, so the recording reflects boomerang's default settings
# rather than whatever is currently in Jeff's real config.toml.
#
# The outer canvas is taller than the other two nested-tmux tapes need
# (1150 vs. their 950/600) because this is the one recording that has to
# fit boomerang's full edit form - Title, Body, Labels, Submit - inside the
# popup's "-h 60%" slice rather than a full, unwrapped terminal the way the
# retired edit-issue.tape rendered it. Re-check this if the edit form ever
# grows a field.
#
# Story: this is boomerang's single richest demo, folding what used to be
# two separate recordings (browse-and-yank.tape, edit-issue.tape, both now
# retired) into one continuous, plausible pass through the app: the
# recording opens on a bare, already-waiting shell prompt - nothing
# happened yet. The full command line is typed out, closing quote and all:
# `claude -p "let's implement these issues as a bundle: "` - the way a
# shell with auto-paired quotes would leave it. Typed in a few chunks at
# varying TypingSpeed (see the vhs-recording skill's "vary typing speed"
# step) rather than one uniform speed, since a constant per-character delay
# reads as obviously scripted the moment there's a full sentence on screen.
# The cursor then steps left, back inside the quotes, before the popup
# opens - this is what makes the eventual paste land in the middle of the
# line instead of magically appearing after a quote that wasn't there yet.
#
# The popup (Ctrl+B i) opens over that half-typed command and scrolls down
# through the list to the last, unlabeled issue - the same one
# quick-capture.gif shows getting filed with nothing but a title, picked
# back up here to be fleshed out. `e` opens the edit form; rather than
# retyping the whole rationale (that's edit-issue.tape's retired story),
# this pass keeps the edit itself light - tabbing straight past Title and
# Body to the Labels field and toggling on "spike", since a stray idea
# turning out to warrant a time-boxed investigation is exactly the kind of
# small, honest edit someone actually makes when they reopen an old
# capture. Submitting lands back on the list with that label now showing.
#
# From there the story becomes the multi-select yank: `Space` checks the
# freshly-labeled issue, then the cursor moves up to "Explain why light has
# a speed limit" - a genuinely related issue, not an arbitrary pairing -
# and checks that one too. `y` yanks both as one comma-joined string,
# closing lands control right back in the still-half-typed shell command
# with the cursor still parked before the closing quote, and pasting drops
# the numbers straight in.

Output "docs/images/hero.gif"

Set Shell zsh
Set FontSize 14
Set Width 1100
Set Height 1150
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

Down@250ms 4
Sleep 700ms

Type "e"
Sleep 900ms

Tab
Sleep 300ms
Tab
Sleep 400ms
Down@150ms 4
Sleep 400ms
Space
Sleep 600ms
Tab
Sleep 400ms
Enter
Sleep 1800ms

Space
Sleep 700ms
Up@250ms 4
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
