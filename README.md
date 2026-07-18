# boomerang

A tmux-popup TUI for browsing, searching, creating, editing, and closing
GitHub issues in the repo sitting in your current pane, without leaving the
terminal. Same architectural family as
[smux](https://github.com/jeffdt/smux): a small Rust binary launched on
demand via `tmux display-popup`.

## Install

Build from source:

```sh
cargo build --release
```

Then add a keybind to `~/.tmux.conf`, pointing at the built binary (or a copy
on your `PATH`):

```tmux
bind i display-popup -E -B -d "#{pane_current_path}" -w 84 -h 60% "exec boomerang"
```

The `-d "#{pane_current_path}"` matters: without it, `display-popup` doesn't
reliably inherit the current pane's working directory, so `gh` ends up
running outside your repo and the popup exits (and closes) immediately.

Reload tmux and press `prefix + i`. Popup dimensions are a starting point,
not fixed — the create/edit form may want more vertical room than the list
view; adjust `-w`/`-h` to taste.

For instant title-only capture without opening the full list (see
`--capture` under Quick capture below), bind a second key to a much shorter
popup. Unlike the rest of the app, the quick-create prompt is a deliberately
distinct compact screen: it sits flush at the top with no margin, and its
hint row doubles as the status line (showing the create-in-progress/error
message in place of the hint rather than reserving a separate row for it),
so it's a fixed 4 rows tall — the popup itself only needs to be exactly
that tall:

```tmux
bind I display-popup -E -B -d "#{pane_current_path}" -w 84 -h 4 "exec boomerang --capture"
```

## How it works

- Auto-detects the repo from the current directory via `gh`'s own git-remote
  detection — no config or `--repo` flag needed.
- Opens the TUI immediately with a rotating loading animation while issues and
  labels are fetched in the background.
- Fetches all open issues (including body and labels) in one `gh issue list`
  call, so the description pane never needs a follow-up network call.
- Create and edit submissions run in the background with an in-place pending
  indicator, then refresh the issue list when `gh` returns.
- All GitHub interaction shells out to the `gh` CLI, which must be installed
  and authenticated (`gh auth login`).

## Keys

| Key | Action |
| --- | --- |
| `j`/`k` (or `↓`/`↑`) | Move the cursor |
| `h` | Hide/show the description pane (shown by default) |
| `/` | Fuzzy search by title (`Enter`/`Esc` to return to the list) |
| `a` | Cycle state filter: open → closed → all |
| `c` | Little create: title only, created immediately |
| `C` (shift+c) | Big create: title + body + label picker |
| `Enter` / `e` | Edit the selected issue's title/body/labels |
| `x` | Close the selected issue (y/n confirm) |
| `y` | Copy `#123` to the clipboard |
| `Y` (shift+y) | Copy a markdown link to the clipboard |
| `Ctrl-y` | Copy the plain URL to the clipboard |
| `,` | Open Settings |
| `q` / `Esc` | Quit |

Inside the create/edit form: `Tab`/`Shift+Tab` moves between Title/Body/Labels,
`Space` toggles a label when the Labels field is focused, and `Enter` advances
Title → Body → submit (submitting from the Labels field).

## Settings

Press `,` to open Settings, a small view of picker-wide preferences. `j`/`k`
(or `↓`/`↑`) moves between rows, `Enter`/`Space`/`h`/`l` toggles the selected
row, and `q`/`Esc` returns to the list.

| Setting | Default | Description |
| --- | --- | --- |
| Exit popup after copy/yank | Off | When on, a successful `y`/`Y`/`Ctrl-y` copy closes the popup immediately instead of staying open. |
| Zebra striping | On | Dims every other row in the issue list to make scanning easier. Uses your terminal's own faint/dim rendering rather than a fixed color, so it adapts to your terminal theme. |

## Quick capture

`boomerang --capture` skips the list view entirely and opens straight to
the title-only quick-create prompt (`Enter` to create, `Esc` to cancel),
then exits — handy bound to its own key (see the `bind I` example above) for
firing off an issue without leaving your current pane's context. The prompt
shows the repo it'll create the issue in once `gh repo view` resolves in the
background. `boomerang --capture-full` does the same but opens the full
title/body/label form instead.

## Diagnostics

Run `boomerang --doctor` from the target repo to print cwd, git remote,
`gh` auth, detected GitHub repo, token-env, and diagnostic logging state.

`gh` follows normal environment precedence. If tmux exports `GITHUB_TOKEN`, that
token overrides the `gh` keyring account. For personal repos where your shell has
a work token, launch with:

```sh
env -u GITHUB_TOKEN boomerang
```

Opt-in command diagnostics by setting `BOOMERANG_LOG=1`. Logs go to
`~/.cache/boomerang/boomerang.log` by default, or to
`BOOMERANG_LOG_PATH` when set. Logs include sanitized `gh` argv, elapsed
milliseconds, exit status, stdout byte count, and stderr. Issue titles and bodies
passed to `gh` are redacted.

Startup defaults to the Matrix rain animation because it reads best during the
brief initial load. Set `BOOMERANG_LOADING_ANIMATION=ripple` to try the
experimental color-ripple bullseye loader, or `rainbow` for continuous thick
color-locked bands.

Preview a loader without touching `gh`:

```sh
boomerang --preview-loading matrix 10s
boomerang --preview-loading ripple 1500ms
boomerang --preview-loading rainbow 10s
```

Bare durations are seconds, so `boomerang --preview-loading 10` previews the
default loader for ten seconds.

## Disclaimer

Early, single-purpose personal tool. Use at your own risk.

## License

MIT
