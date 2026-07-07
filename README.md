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
bind i display-popup -E -B -d "#{pane_current_path}" -w 84 -h 60% "exec issue-browser"
```

The `-d "#{pane_current_path}"` matters: without it, `display-popup` doesn't
reliably inherit the current pane's working directory, so `gh` ends up
running outside your repo and the popup exits (and closes) immediately.

Reload tmux and press `prefix + i`. Popup dimensions are a starting point,
not fixed — the create/edit form may want more vertical room than the list
view; adjust `-w`/`-h` to taste.

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

## Diagnostics

Run `issue-browser --doctor` from the target repo to print cwd, git remote,
`gh` auth, detected GitHub repo, token-env, and diagnostic logging state.

`gh` follows normal environment precedence. If tmux exports `GITHUB_TOKEN`, that
token overrides the `gh` keyring account. For personal repos where your shell has
a work token, launch with:

```sh
env -u GITHUB_TOKEN issue-browser
```

Opt-in command diagnostics by setting `ISSUE_BROWSER_LOG=1`. Logs go to
`~/.cache/issue-browser/issue-browser.log` by default, or to
`ISSUE_BROWSER_LOG_PATH` when set. Logs include sanitized `gh` argv, elapsed
milliseconds, exit status, stdout byte count, and stderr. Issue titles and bodies
passed to `gh` are redacted.

Set `ISSUE_BROWSER_LOADING_ANIMATION` to `matrix`, `pipes`, `starfield`,
`black-hole`, or `bonsai` to pin a specific startup animation while
experimenting. Leave it unset to rotate.

## Disclaimer

Early, single-purpose personal tool. Use at your own risk.

## License

MIT
