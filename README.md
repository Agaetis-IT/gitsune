# git-hydra

A desktop git client built around a **metro-style branch graph**: one colored line per branch, commits as stations, merges as interchanges. Browsing history, searching and reading diffs are complete. Writing is being added step by step (version 2): **staging by file and hunk, committing and amending, discarding with undo, branches, checkout, tags, fetch, pull and push** are in; merge and rebase are planned (see [SPEC.md](SPEC.md), section 12). Every change the app makes goes through the `git` binary and is listed in an activity log; a read-only switch per repository refuses all writes.

Tauri 2 (Rust) + Vue 3 + TypeScript + Tailwind + shadcn-vue. macOS first.

## Features

- Metro graph on a virtualized canvas, comfortable up to ~100,000 commits.
- Tabs per repository, recent repositories, system light/dark theme.
- Sidebar with local branches, remotes and tags: hide a branch or a group, highlight a branch, jump to its commit.
- Click a lane in the graph to highlight or hide its branch.
- Search commits by message, author or id (Ctrl/Cmd+F).
- Commit details, changed files, unified and side-by-side diffs with syntax highlighting; large and binary files are handled.
- Live refresh: commits, fetches and checkouts made in a terminal appear without losing your place; changes to the working tree show up in the status as you save files.
- Stage files or single hunks, commit with a summary and description (hooks and signing behave as in the terminal), amend, discard changes with an undo, and an activity log of every git command the app ran.
- Create, rename and delete branches, set upstreams, switch branches (with a stash-or-discard choice when local changes are in the way), create and delete tags, all from context menus on the graph and the sidebar; deletions can be undone.
- Fetch, pull (merge or rebase) and push (with an optional `--force-with-lease`), with progress, cancel and password prompts through your usual credential helpers and ssh-agent.

## Requirements

- macOS, [Bun](https://bun.sh), a Rust toolchain (edition 2024, Rust 1.85 or newer) and the Xcode command line tools.
- `git` on the path, only for the tests and the synthetic repository generator.

## Everyday commands

```bash
bun install
bun run tauri dev        # the app, with hot reload
bun run dev              # the UI alone in a browser, on recorded demo data (no Tauri needed)
bun run check:all        # typecheck, lint, format, front tests, rustfmt, clippy, Rust tests
bun run package          # release build: .app and .dmg under src-tauri/target/release/bundle
```

In the plain browser mode the app shows a small invented repository. In the browser console, `__simulateRepoChange()` adds a commit on top, like the file watcher does for a real repository.

The release build is not signed or notarized: it runs on the machine that built it; to share it, sign it first.

## Large repositories

The synthetic generator builds a repository of invented commits (fake authors, no real data):

```bash
bun scripts/gen-synthetic-repo.ts /tmp/synth-100k 100000
cd src-tauri
SYNTH_REPO=/tmp/synth-100k cargo test --release -- --ignored --nocapture
```

The ignored tests print timings for the layout, paging, search and refresh. `docs/benchmarks/bench_git.rs` is the original `gix` vs `git2` comparison.

## Layout of the code

| Path                              | What                                                                                                   |
| --------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `src-tauri/src/layout`            | Lane layout engine: topological order, lanes, connectors, names, colors                                |
| `src-tauri/src/git`               | Read-only git access (`gix`): refs, graph cache and pages, commit details, diffs, search, file watcher |
| `src-tauri/src/commands.rs`       | Tauri commands and the registry of open repositories                                                   |
| `src/lib/metro`                   | Canvas renderer: geometry, drawing, palette, row cache                                                 |
| `src/composables/useWorkspace.ts` | Application state: tabs, selection, search, refresh                                                    |
| `src/components`                  | Vue components (graph, sidebar, commit panel, diff view...)                                            |
| `src/dev`                         | Demo data and stand-in backend for browser development (not in production builds)                      |
| `scripts`                         | Synthetic repository and icon generators                                                               |

All design decisions, measurements and the milestone plan are in [SPEC.md](SPEC.md).

## Not included

Merge and rebase (planned, see the spec), GitHub/GitLab integrations, conflict resolution, submodules and worktrees, file editing.
