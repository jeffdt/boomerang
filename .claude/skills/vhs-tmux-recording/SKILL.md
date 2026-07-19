---
name: vhs-tmux-recording
description: >
  Use whenever the task is to record, re-record, or update a GIF or video
  demo of a tmux-based tool — a tmux popup, plugin, or any TUI driven from
  inside tmux — using vhs (charmbracelet/vhs). Also use any time a script or
  `.tape` file needs to run `tmux` commands to set up or drive a recording,
  even outside of vhs specifically, because the isolation rules here are
  what stop a scripted tmux command from accidentally hitting the real,
  currently-attached tmux server instead of a throwaway one. Trigger this
  before writing a `.tape` file, before running `tmux new-session` /
  `tmux -L` / `tmux kill-server` as part of any recording or screenshot
  task, and before regenerating an existing demo asset (e.g. "update the
  README gif", "the popup UI changed, can you re-record the demo",
  "record a quick screencast of prefix+X"). Running tmux automation without
  following this skill has previously torn down a real, in-use tmux server
  and every session inside it — treat that as a live risk, not a
  hypothetical.
---

# Recording tmux tools with vhs

Records of tmux popups and other tmux-driven TUIs are trickier than a
normal terminal recording: the thing you're demoing is chrome that only
exists *inside a running tmux server* (`display-popup`, a status line, a
pane layout). vhs alone gives you a clean pty to script, but it doesn't
know anything about tmux — so getting the real chrome on screen means
scripting a real `tmux` session from inside the recording. That's where
the risk lives: every `tmux` command without an explicit socket targets
whichever server is ambient, including the one the user (or you) are
currently attached to.

## The one rule that matters

**Every `tmux` invocation used for a recording must carry `-L
<isolated-socket-name>`, with no exceptions, including while debugging a
tape by hand.** `tmux -L <name>` creates a fully separate server and socket
file, independent of the `default` socket. Without it, `tmux new-session`,
`tmux kill-server`, or even innocuous-looking commands run against whatever
server the shell's `$TMUX` happens to point at — which, if you're iterating
inside an actual tmux session (very likely, since that's how most people
run a terminal), is the real one. A `kill-server` issued that way doesn't
just clean up the recording's throwaway session, it kills *every* real
session on the machine, including the one you're working in. This has
happened before during this kind of work; it is the reason this skill
exists.

Two concrete habits that follow from this:

- Pick one memorable socket name per project (e.g. `boomerang-demo-gif`)
  and use it for every `tmux` command in the tape, with no bare `tmux ...`
  calls anywhere.
- Also `unset TMUX` at the start of the tape's setup, before creating the
  isolated session. If the outer shell vhs launches happens to inherit a
  `$TMUX` from its own environment, some tmux subcommands will trust that
  over the explicit `-L` flag in surprising ways; clearing it removes the
  ambiguity entirely.

## Workflow

### 1. Decide what the recording needs to prove

Write down the one thing a viewer should walk away understanding (e.g.
"capturing an idea is fast enough that you don't lose your place in what
you were doing"). This drives two decisions below: whether the recording
needs real tmux chrome at all, and what should be visible on screen before
the interesting part starts. A recording that opens on a blank prompt and
then does the interesting thing reads as a feature demo; one that opens
mid-task and the interesting thing happens *as an interruption* reads as
"look how little this costs you" — pick deliberately, don't default to
blank-prompt-then-go.

### 2. Handle any real side effects the recording will cause

If the interaction under test does something real and externally visible
(creates a GitHub issue, sends a message, writes a file to a shared
location), decide up front whether to fake it or let it happen for real.
Letting it happen for real is usually more honest on screen (no fear of a
mocked UI looking subtly wrong), but don't let it land in a tracker or
system anyone actually uses for real work. If no safe target already
exists, create one: a dedicated, disposable resource that exists solely to
receive recording side effects (a private sandbox repo is the usual shape
for GitHub-issue-creating tools). Once it exists, point every future
recording at it and never worry about cleanup — that's the point of it
being disposable. Don't reuse a real project's tracker "just this once and
I'll close it after"; that habit doesn't scale to re-recordings after every
UI change.

### 3. Write the `.tape` file

Structure:

```
Output "path/to/output.gif"

Set Shell zsh
Set FontSize 14
Set Width <w>
Set Height <h>
Set Theme "<theme-name>"        # optional, see vhs's bundled theme list

Hide
Type "unset TMUX; tmux -L <isolated-socket> -f <tmux-config> new-session -s <name> <shell>"
Enter
Sleep 2s
# ... any other setup that shouldn't appear in the final recording ...
Show

# ... the actual scripted interaction, this part is what gets recorded ...

Hide
Type "exit"
Enter
```

Notes:

- `Hide` / `Show` let you execute real commands (setup, teardown) without
  them appearing in the rendered output, while still running in real time
  against the real pty. Use `Hide` liberally for anything that's plumbing
  rather than the demo itself — typing out a long setup command on screen
  both looks messy and burns the recording's time budget before the actual
  content appears.
- If the tmux config file path is needed after a `cd`, capture it as a
  shell variable *before* the `cd` (e.g. `REPO=$PWD` at the very start),
  then reference `$REPO/...`. A relative path typed after `cd`-ing
  elsewhere silently resolves against the wrong directory — this is an
  easy mistake to make and easy to miss because the tape doesn't error, it
  just doesn't show what you expect.
- Keep the isolated session's tmux config (keybinds etc.) as its own
  checked-in file alongside the tape, mirroring whatever the tool's real
  keybind looks like in a user's actual dotfiles, so the recording
  exercises the real chrome rather than a simplified stand-in.

### 4. Watch for terminal-theme conflicts

`Set Theme` remaps vhs's 16 base ANSI colors. It has no effect on anything
that emits truecolor (24-bit RGB) escape codes directly — most themed
shell prompts (Starship, Oh My Posh, etc.) do exactly that, so a themed
prompt will visibly fight whatever theme the tape requests. If the
recorded pane's prompt looks like it's not respecting `Set Theme`, this is
almost always why. Fix it by launching the pane's shell without loading
its normal rc files — `zsh -f` for zsh, `bash --norc --noprofile` for
bash — so no prompt tool initializes at all. This also has the side
benefit of a clean, fast-starting shell with no personal aliases or
functions that could shadow something the tape depends on.

### 5. Run it and inspect the result

```sh
vhs path/to/file.tape
```

Preview the resulting GIF with real animation rather than a static first
frame — macOS Preview.app only shows one frame; `qlmanage -p <path>` (Quick
Look) plays it. To inspect specific moments frame-by-frame (useful for
diagnosing timing issues), extract frames with ffmpeg:

```sh
ffmpeg -i output.gif -vf "fps=4" frame-%03d.png
```

then read the relevant PNGs directly.

### 6. Verify the isolated session actually tore down

Don't assume the tape's teardown worked just because vhs exited cleanly.
If a scripted "exit" step doesn't land as expected — `Ctrl+D` failing to
register as EOF inside a REPL is a real failure mode, not a hypothetical
one — the isolated tmux server keeps running after vhs's own process
exits. That's harmless in isolation, but it silently breaks the *next*
recording: `tmux new-session -s <name>` collides with the leftover session
(`duplicate session: <name>`), the tape's subsequent keystrokes fall
through to whatever that stale session happens to be running instead of
the fresh one the tape assumes, and the result is a broken take that looks
like garbled input rather than an obvious error.

After every run:

```sh
tmux -L <isolated-socket> ls
```

should fail with "no server running" (or similar). If a server is still
there, `tmux -L <isolated-socket> kill-server` is always safe to run — it's
scoped to that one socket and cannot touch the default server or any real
session, no matter what's running elsewhere on the machine. Prefer an
explicit, unambiguous exit sequence in the tape's teardown (e.g. exiting an
inner REPL with its own explicit exit call, then exiting the outer shell)
over relying on a single `Ctrl+D` to cascade through multiple nested
programs.

## Worked example

boomerang's own `docs/demo/quick-capture.tape` and
`docs/demo/demo.tmux.conf` are a complete, checked-in reference
implementation of everything above: isolated nested tmux server, real
`display-popup` chrome, a disposable sandbox repo for the real GitHub
issue it creates, Gruvbox theming with `zsh -f` to suppress a themed
prompt, and an explicit multi-step teardown. Read them for a concrete
example rather than starting from a blank tape. Regenerate it with:

```sh
vhs docs/demo/quick-capture.tape
```

run from the repo root (prerequisites are documented in the tape's own
header comment).

## Checklist recap

- [ ] Every `tmux` command carries `-L <isolated-socket>`; `$TMUX` is
      unset before the isolated session is created.
- [ ] Any real side effect lands in a disposable sandbox, never a real
      tracker or shared system.
- [ ] Setup/teardown plumbing is wrapped in `Hide`/`Show` so only the
      actual demo renders.
- [ ] The pane's shell skips rc files (`zsh -f` / `bash --norc`) if a
      themed prompt would otherwise fight `Set Theme`.
- [ ] After running, `tmux -L <isolated-socket> ls` reports no server —
      teardown actually happened, not just "vhs exited."
