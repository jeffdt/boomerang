# issue-browser

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
bind i display-popup -E -B -w 84 -h 60% "exec issue-browser"
```

Reload tmux and press `prefix + i`. Popup dimensions are a starting point,
not fixed — the create/edit form may want more vertical room than the list
view; adjust `-w`/`-h` to taste.

## How it works

- Auto-detects the repo from the current directory via `gh`'s own git-remote
  detection — no config or `--repo` flag needed.
- Fetches all open issues (including body and labels) in one `gh issue list`
  call, so the description pane never needs a follow-up network call.
- All GitHub interaction shells out to the `gh` CLI, which must be installed
  and authenticated (`gh auth login`).

## Keys

| Key | Action |
| --- | --- |
| `j`/`k` (or `↓`/`↑`) | Move the cursor |
| `Enter` | Toggle the description pane for the selected issue |
| `/` | Fuzzy search by title (`Enter`/`Esc` to return to the list) |
| `a` | Cycle state filter: open → closed → all |
| `c` | Little create: title only, created immediately |
| `C` (shift+c) | Big create: title + body + label picker |
| `e` | Edit the selected issue's title/body/labels |
| `x` | Close the selected issue (y/n confirm) |
| `y` | Copy `#123` to the clipboard |
| `Y` (shift+y) | Copy a markdown link to the clipboard |
| `Ctrl-y` | Copy the plain URL to the clipboard |
| `q` / `Esc` | Quit |

Inside the create/edit form: `Tab`/`Shift+Tab` moves between Title/Body/Labels,
`Space` toggles a label when the Labels field is focused, and `Enter` advances
Title → Body → submit (submitting from the Labels field).

## Disclaimer

Early, single-purpose personal tool. Use at your own risk.

## License

MIT
