# AGENTS.md

Orientation for agents and humans working on boomerang. This file holds
durable intent and conventions, not a file-by-file map (that goes stale).
Read the source for current structure.

## What this is

boomerang is a terminal UI for browsing, searching, creating, editing, and
closing GitHub issues in the repo sitting in the current directory. It is a
standalone compiled binary that tmux launches on demand via `tmux popup -E`;
it is not a tmux plugin and runs no background process. Same architectural
family as its sibling project, [rolomux](https://github.com/jeffdt/rolomux)
(formerly `smux`).

## Claude Code plugin

Shared skills (`mockup`, `vhs-recording`, `cutting-a-release`, `live-preview`)
come from the `tui-utils` plugin (github.com/jeffdt/tui-utils), not this
repo. One-time per machine, at user scope:

```
/plugin marketplace add jeffdt/tui-utils
/plugin install tui-utils@tui-utils
```

This repo's per-app modifiers for those skills live in
`[package.metadata.tui-utils]` in `Cargo.toml` (keys documented in the
plugin's README).

## Durable design decisions

- **Mock up visual/rendering changes before writing the spec.** When a design
  discussion touches how something renders (colors, layout, new
  glyphs/columns), don't rely on a text description alone: build a mockup
  with the `mockup` skill so Jeff can look at it before design gets locked
  in. Skip this for changes with no visual surface (model/logic-only work).
- **Leave a live preview when a feature is done** with the `live-preview`
  skill. Any manual run of the binary also points `XDG_CONFIG_HOME` at a
  scratch directory, since boomerang has no isolation flag of its own and
  would otherwise mutate the real `~/.config/boomerang/config.toml`.
- **Named ANSI colors only.** Use the 16 named terminal colors (e.g.
  `Color::Cyan`, `Color::DarkGray`), never `Color::Rgb`. This is what lets the
  picker inherit the user's terminal theme rather than imposing fixed colors.
- **Plan approval is the quality gate, not spec approval.** When using the
  brainstorming skill in this repo, skip the "user reviews written spec"
  checkpoint — go straight from a written spec into the implementation plan.
  Jeff reviews the plan, not the spec, before implementation starts.
- **Check for bundleable issues when picking up a new one.** Before
  brainstorming a requested issue, skim the other open issues for ones that
  share the same area of code, were filed the same day, or carry matching
  labels (e.g. the same `priority`+`small` pair) — fold those into the same
  spec/plan/PR instead of leaving them for a separate pass. If it's a close
  call whether something belongs in scope, ask rather than guessing.
- **Changes land via pull request, never a local merge to `main`.** Push the
  feature branch and open a PR, then merge it yourself (squash, to keep
  `main` linear) — this is a solo project with no human review gate, so the
  PR exists for release notes, not approval. Release notes are
  auto-generated from merged PRs; a local `git merge` still ships the change
  (it's in the commit history either way) but silently drops it from the
  next release's "What's Changed" list.

## Packaging and distribution

boomerang ships as a prebuilt binary through a personal Homebrew tap,
mirroring the `rolomux`/`teleport` pattern:

- A `v*` git tag triggers `.github/workflows/release.yml`, which builds the
  `aarch64-apple-darwin` binary and attaches it to the GitHub Release.
- `jeffdt/homebrew-tap` carries `Formula/boomerang.rb`, a binary formula that
  downloads that asset by pinned `sha256`. Install with
  `brew install jeffdt/tap/boomerang`.
- **The tmux keybind is not part of the package.** It lives in the user's
  dotfiles (`~/.tmux.conf`), e.g.
  `bind i display-popup -E -B -d "#{pane_current_path}" -w 84 -h 60% "exec boomerang"`.
  Distribution ships the binary; the bind travels with the user's config.

### Cutting a release

**Every push to `main` that changes shipped behavior must also cut a
release** (Homebrew users never see `main`, only tagged releases). See the
`cutting-a-release` skill for the full bump/tag/tap workflow; set
`BOOMERANG_TAP_DIR` if the tap isn't at `~/code/homebrew-tap`.

## Regenerating the README demo GIFs

The README's three demo GIFs (`docs/images/hero.gif`, `quick-capture.gif`,
`settings.gif`) are generated, not hand-recorded. Re-run the relevant tape
after any visible change to that flow so the README doesn't go stale:

```sh
docs/demo/seed-issues.sh          # always run first, see below
vhs docs/demo/hero.tape
vhs docs/demo/quick-capture.tape
vhs docs/demo/settings.tape
```

Run from the repo root. Full prerequisites and mechanics are documented in
each tape file's own header comment; the short version: `hero.tape` and
`quick-capture.tape` both nest a real, isolated tmux server inside the
recording so the actual `display-popup` chrome renders (not just
boomerang's own UI) — the popup interrupting a real shell is the point of
both. `hero.tape` is the flagship recording, shown right under the
README's intro: it folds what used to be two separate tapes
(`browse-and-yank.tape`, `edit-issue.tape`, both retired) into one
continuous pass — scroll the list, edit an issue's labels, then check and
yank it plus a related issue in one go — because a single richer demo of
the full issue browser sells the app's depth far better than a
minimal-but-narrow capture flow ever could. `settings.tape` doesn't need
tmux chrome, since it only shows boomerang's own Settings view, so it runs
directly in the recorded shell. All three submit real writes to
`jeffdt/universe` (a private sandbox repo that exists solely for this — no
cleanup needed after) and run the pane's shell as `zsh -f` so prompt tools
like Starship don't emit truecolor escapes that fight the recording's
chosen `Set Theme`.

**Run `docs/demo/seed-issues.sh` before recording.** It resets
`jeffdt/universe` to a known-good state: reconciles the repo's labels down
to a small curated set (`bug`, `docs`, `feature`, `good first issue`,
`spike` — anything else, including GitHub's own defaults, gets deleted),
reconciles the filler issues `hero.tape` needs (title, labels, and body all
get overwritten to match on every run), closes duplicate "Check if light
speed is constant for every observer" issues left over from prior
recordings (keeping one, reset to a pristine no-body/no-labels state since
`hero.tape` labels it live), and closes every open "spike: check if light
speed is constant for every observer" issue — the real, uncapped byproduct
`quick-capture.tape` files on every run, which nothing else ever cleans up.
It's idempotent, safe to run before every recording regardless of current
state. If you change any of the titles/labels/bodies, edit
`seed-issues.sh` and re-run it rather than hand-editing issues in
`jeffdt/universe` directly — otherwise the next recording session silently
reverts your edit back to whatever the script says.

**Isolate `XDG_CONFIG_HOME` too, and preserve `gh`'s auth when you do.**
Each tape exports `XDG_CONFIG_HOME` to a scratch directory before launching
boomerang, so the recording always reflects boomerang's *default* settings
rather than whatever's currently in Jeff's real `config.toml` — this bit
Jeff once already, where `exit_on_copy_yank = true` in the real config made
boomerang quit immediately after the first `y` press mid-recording, cutting
a take short in a way that wasn't obvious from the tape itself. `gh` also
honors `XDG_CONFIG_HOME`, so blindly overriding it strips `gh`'s real auth
too; every tape symlinks the real `~/.config/gh/*.yml` into the scratch
directory to avoid that.

**The tmux isolation in `hero.tape` and `quick-capture.tape` is
load-bearing, never drop it.** Their `tmux -L boomerang-demo-gif ...` is not
incidental — that flag is what keeps the recording's nested tmux server
from touching Jeff's actual one. Running `tmux` commands against the
default socket during a recording session (e.g. while debugging a tape by
hand) and then issuing something like `kill-server` takes down every real
tmux session on the machine, not just the throwaway one, since sessions
aren't isolated by which pty invoked them, only by socket. This has
happened before.

**Verify the isolated tmux session actually tore down after recording
either nested tape.** `quick-capture.tape` exits its nested shell with
scripted keystrokes (e.g. `exit()` then `exit`); `hero.tape` with a single
`exit`. If a step in that chain doesn't land as expected — `Ctrl+D` not
registering as EOF inside a REPL has already happened once — the isolated
server is left running instead of exiting on its own. That's normally
harmless in isolation, but the *next* recording's `tmux new-session -s
demo` then fails with `duplicate session: demo` against the leftover one,
silently drops out of any tmux context, and produces a broken take
(keystrokes meant for the popup get typed as literal garbage into
whatever's still running). After every run: `tmux -L boomerang-demo-gif
ls` should report no server. If one is lingering, `tmux -L
boomerang-demo-gif kill-server` is always safe to clean it up — it's
scoped to that one socket, never the default one.

**`clear` the screen right before launching boomerang in `settings.tape`.**
It's the only tape left that runs boomerang directly rather than inside
tmux, and boomerang is a fullscreen (alt-screen) TUI — when it quits, the
terminal restores whatever the primary screen buffer looked like before it
launched. `Hide` only skips *rendering* the hidden setup commands, it
doesn't erase them from the pty's actual scrollback, so without a `clear`
right before `Show`/launching boomerang, the whole setup one-liner
reappears the moment boomerang exits back to the shell. The two nested-tmux
tapes don't need this — tmux's own alt-screen swallows the setup and the
recording never returns to the outer shell to reveal it.

**The Labels field in boomerang's edit form doesn't scroll to follow the
cursor** (`render_labels` in `src/ui.rs` renders a plain ratatui `List` with
no `ListState`, so the visible window is always the first N items
regardless of where the cursor actually is — worth fixing in boomerang
itself at some point). `jeffdt/universe`'s curated 5-label set (see above)
currently fits on screen in its entirety regardless of cursor position, so
this doesn't bite `hero.tape` today. If the label set grows again, re-check
that whichever label the tape selects (currently "spike", the last one
alphabetically) is still visible in the recorded frame, or the selection
will silently happen off-screen.

**`hero.tape`'s outer canvas is taller than `quick-capture.tape`'s (1150
vs. 600 rows)** because it's the one recording that has to fit boomerang's
full edit form — Title, Body, Labels, Submit — inside the popup's `-h 60%`
slice rather than a full, unwrapped terminal the way the retired
`edit-issue.tape` rendered it. Re-check this if the edit form ever grows a
field, or if the popup's own `-h` in the README's `bind i` example changes.
