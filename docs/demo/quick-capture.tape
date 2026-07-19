# Regenerates docs/images/quick-capture.gif for the README.
#
# Prerequisites:
#   - Run from the repo root: vhs docs/demo/quick-capture.tape
#   - `boomerang` on PATH (the real binary, not a dev build)
#   - `gh` authenticated
#   - python3 on PATH (for the background "work" the recording opens on)
#   - jeffdt/universe cloned at ~/code/universe — a private sandbox repo that
#     exists solely to receive throwaway issues from recordings like this one.
#     Nothing here needs cleanup afterward.
#
# Nests a real, isolated tmux server (its own socket, never the default one)
# inside the recording so the actual `display-popup` chrome renders, not just
# boomerang's own UI. The pane runs `zsh -f` (no rc files) so prompt tools
# like Starship don't load — those often emit truecolor escapes that bypass
# `Set Theme` entirely, which would fight the chosen theme on screen.
#
# The recording opens mid-flow, not on a blank prompt: hidden setup builds up
# a python REPL already a few steps into an unrelated Newtonian orbital-
# velocity calculation, then playback becomes visible right as that result
# lands. The captured idea (light speed being constant) is deliberately
# unrelated to the on-screen math — it's a tangent, not a conclusion the
# calculation proves, since the point is capturing a stray thought without
# losing your place in whatever you were actually doing, not formalizing
# something you just derived. The popup closes right back into the still-
# running REPL to sell exactly that.

Output "docs/images/quick-capture.gif"

Set Shell zsh
Set FontSize 14
Set Width 1100
Set Height 600
Set Theme "GruvboxDark"

Hide
Type "unset TMUX; BOOMERANG_REPO=$PWD; cd ~/code/universe && tmux -L boomerang-demo-gif -f $BOOMERANG_REPO/docs/demo/demo.tmux.conf new-session -s demo zsh -f"
Enter
Sleep 2s

Type "python3"
Enter
Sleep 1s

Type "G = 6.674e-11  # gravitational constant"
Enter
Sleep 300ms
Type "M = 5.972e24  # earth mass, kg"
Enter
Sleep 300ms
Type "r = 6.371e6  # earth radius, m"
Enter
Sleep 300ms
Show

Sleep 400ms
Type "v = (G * M / r) ** 0.5  # orbital velocity"
Enter
Sleep 600ms
Type "v"
Enter
Sleep 1500ms

Ctrl+B
Type "I"
Sleep 900ms

Type "spike: check if light speed is constant for every observer"
Sleep 500ms
Enter
Sleep 3s

Hide
Type "exit()"
Enter
Sleep 300ms
Type "exit"
Enter
Sleep 500ms
