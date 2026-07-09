# AGENTS.md

Orientation for agents and humans working on issue-browser. This file holds
durable intent and conventions, not a file-by-file map (that goes stale).
Read the source for current structure.

## What this is

issue-browser is a terminal UI for browsing, searching, creating, editing, and
closing GitHub issues in the repo sitting in the current directory. It is a
standalone compiled binary that tmux launches on demand via `tmux popup -E`;
it is not a tmux plugin and runs no background process. Same architectural
family as its sibling project, [rolomux](https://github.com/jeffdt/smux)
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
  before touching code — don't ask first, just do it, then mention it. This
  is still a solo project, so there's no need for PRs on routine work; merge
  the worktree branch back into `main` (or fast-forward it) once the change
  is finished and verified, unless Jeff asks for a PR explicitly.
