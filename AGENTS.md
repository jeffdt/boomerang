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

## Durable design decisions

- **Mock up visual/rendering changes before writing the spec.** When a design
  discussion touches how something renders (colors, layout, new
  glyphs/columns), don't rely on a text description alone — render an ANSI
  mockup (a small script with `printf`/`echo -e` escape codes, not the real
  binary) in a new tmux window via `mux spawn --workspace caller`, so Jeff can
  look at it before design gets locked in. Skip this for changes with no
  visual surface (model/logic-only work).
- **Named ANSI colors only.** Use the 16 named terminal colors (e.g.
  `Color::Cyan`, `Color::DarkGray`), never `Color::Rgb`. This is what lets the
  picker inherit the user's terminal theme rather than imposing fixed colors.
- **Plan approval is the quality gate, not spec approval.** When using the
  brainstorming skill in this repo, skip the "user reviews written spec"
  checkpoint — go straight from a written spec into the implementation plan.
  Jeff reviews the plan, not the spec, before implementation starts.
- **Always work in a worktree; never implement directly on `main`.** Before
  starting any implementation work (not just investigation/Q&A), check
  whether the session is already running in a worktree. If it isn't, create
  one immediately with `wt switch --create jeffdt/<domain>-<brief-description>`
  before touching code — don't ask first, just do it, then mention it.
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
release.** Users install via Homebrew, which only ever sees tagged release
binaries, never `main`. A commit on `main` with no accompanying release is
invisible to anyone who runs `brew upgrade`. So unless a change is purely
internal (docs, tests, CI, scratch under `specs/`/`plans/`), finish the job
in the same session: bump, tag, wait for CI, update the tap. Don't leave
`main` ahead of the latest release.

The version bump rides in the PR that ships the change. Once it has merged,
cut the tag and update the tap. `scripts/release.sh` automates the
mechanical steps (mirrors rolomux's script of the same name); it expects the
tap checked out at `~/code/homebrew-tap` (set `BOOMERANG_TAP_DIR` if it
lives elsewhere):

1. On the feature branch, before opening the PR: `scripts/release.sh bump
   <patch|minor|major>`. Bumps `Cargo.toml`, refreshes `Cargo.lock`, commits.
   That commit rides in the PR.
2. After the PR merges: `git checkout main && git pull`, then
   `scripts/release.sh cut`. Tags and pushes `vX.Y.Z`, waits for
   `release.yml` (builds and attaches **`boomerang-aarch64-apple-darwin`**),
   downloads and hashes the asset, updates and validates
   `jeffdt/homebrew-tap`'s `Formula/boomerang.rb`, pushes the tap, and runs
   `brew update && brew upgrade jeffdt/tap/boomerang` locally.

Two things the script doesn't cover — finish these by hand after `cut`
succeeds:

- If `~/.tmux.conf`'s popup binds were temporarily pointed at a dev build
  (`target/release/boomerang`) for testing, revert them to
  `exec boomerang` and `tmux source-file ~/.tmux.conf`.
- Clean up the worktree once the work is fully shipped: run `wt remove` from
  inside the feature worktree.

Currently Apple Silicon only, matching rolomux.

## Regenerating the README demo GIF

`docs/images/quick-capture.gif` (the README's demo of `prefix+I` quick
capture) is generated, not hand-recorded. Re-run it after any visible change
to the quick-capture flow so the README doesn't go stale:

```sh
vhs docs/demo/quick-capture.tape
```

Run from the repo root. Full prerequisites and mechanics are documented in
the tape file's own header comment; the short version: it nests a real,
isolated tmux server inside the recording so the actual `display-popup`
chrome renders (not just boomerang's own UI), submits one real throwaway
issue to `jeffdt/universe` (a private sandbox repo that exists solely for
this — no cleanup needed after), and runs the pane's shell as `zsh -f` so
prompt tools like Starship don't emit truecolor escapes that fight the
recording's chosen `Set Theme`.

**The tmux isolation is load-bearing, never drop it.** The tape's
`tmux -L boomerang-demo-gif ...` is not incidental — that flag is what keeps
the recording's nested tmux server from touching Jeff's actual one. Running
`tmux` commands against the default socket during a recording session (e.g.
while debugging a tape by hand) and then issuing something like
`kill-server` takes down every real tmux session on the machine, not just
the throwaway one, since sessions aren't isolated by which pty invoked them,
only by socket. This has happened before.

**Verify the isolated session actually tore down after every recording.**
Exiting the tape's nested shell relies on scripted keystrokes (e.g. `exit()`
then `exit`); if a step in that chain doesn't land as expected — `Ctrl+D` not
registering as EOF inside a REPL has already happened once — the isolated
server is left running instead of exiting on its own. That's normally
harmless in isolation, but the *next* recording's `tmux new-session -s demo`
then fails with `duplicate session: demo` against the leftover one, silently
drops out of any tmux context, and produces a broken take (keystrokes meant
for the popup get typed as literal garbage into whatever's still running).
After every run: `tmux -L boomerang-demo-gif ls` should report no server. If
one is lingering, `tmux -L boomerang-demo-gif kill-server` is always safe to
clean it up — it's scoped to that one socket, never the default one.
