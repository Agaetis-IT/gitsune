# Gitsune

A desktop git client built around a **metro-style branch graph**: one colored line per branch, commits as stations, merges as interchanges. Browsing history, searching and reading diffs are complete. Writing is in too (version 2): staging by file and hunk, committing and amending, discarding, branches, checkout, tags, fetch, pull, push, stash, cherry-pick, revert, reset, merge, conflict resolution and rebase (see [SPEC.md](SPEC.md), section 12). Every change the app makes goes through the `git` binary and is listed in an activity log; a read-only switch per repository refuses all writes.

Tauri 2 (Rust) + Vue 3 + TypeScript + Tailwind + shadcn-vue. macOS first.

## Features

- Metro graph on a virtualized canvas, comfortable up to ~100,000 commits. Each station shows the initials of the author on a color of their own (the ring keeps the color of the branch; it can be switched off in Settings).
- Tabs per repository, recent repositories, system light/dark theme.
- Sidebar with local branches, remotes and tags, in folders by the `/` of their names (`feature/auth-18` is in `feature`; a remote's name is its first folder): close and open folders (remembered per repository), hide a branch, a folder or a group, highlight a branch, jump to its commit.
- Click a lane in the graph to highlight or hide its branch.
- Branch and tag labels in their own column on the left of the graph, linked to their station; a laptop for a local branch and an icon per remote (configurable per remote in Settings); resizable columns and optional author / date / id columns.
- The tabs open at the last close come back at start (switch off in Settings, General); on macOS a full menu bar (open and recent repositories, fetch / pull / push, branch actions, view options) with greyed entries when an action makes no sense.
- From a branch: open or create its pull request on GitHub, GitLab or Azure DevOps (signed in with the Azure, GitHub or GitLab CLI, the open one is found: no token to create); open the repository or a file in Zed (or VS Code, Cursor) and the repository in a terminal.
- Themes with the colors of the code in diffs, fonts and base size, import from a file, and a different theme when the system is light or dark.
- Search commits by message, author or id: the _Search_ button or Ctrl/Cmd+F opens a small box at the top right of the graph (Enter and Shift+Enter step through the matches, Esc closes it).
- Commit details, changed files, unified and side-by-side diffs with syntax highlighting; large and binary files are handled.
- Live refresh: commits, fetches and checkouts made in a terminal appear without losing your place; changes to the working tree show up in the status as you save files.
- Stage files or single hunks, commit with a summary and description (hooks and signing behave as in the terminal), amend, discard changes with an undo, and an activity log of every git command the app ran.
- Always know where you are: the row of HEAD is tinted, a _Go to HEAD_ button and an indicator at the edge of the graph take you back to it. Double-click the last commit of a branch (or its label) to switch to that branch.
- Settings (gear at the bottom of the sidebar, or Cmd+,): choose system, light or dark, and a theme among ten (Default, GitHub, Nord, Solarized, Gruvbox, Dracula and the four Catppuccin flavors: Latte, Frappé, Macchiato, Mocha), each with a light and a dark variant that also set the colors of the graph lines. Make your own from a copy, with color pickers and a live preview, and import or export a theme as JSON.
- Create, rename and delete branches, set upstreams, switch branches (with a stash-or-discard choice when local changes are in the way), create and delete tags, all from context menus on the graph and the sidebar; deletions can be undone.
- Fetch, pull (merge or rebase) and push (with an optional `--force-with-lease`), with progress, cancel and password prompts through your usual credential helpers and ssh-agent.
- Stash (list, apply, pop, drop), cherry-pick, revert and reset (soft, mixed, hard), each with a confirmation where something is lost and an Undo.
- Merge (fast-forward, merge commit, squash), with a banner for any merge, cherry-pick, revert or rebase in progress (also when started in a terminal) and a conflict resolver: choose ours, theirs or both per block, edit the result, then continue, skip or abort.
- Rebase onto a branch or a commit, plain or interactive (reorder, reword, edit, squash, fixup, drop), with a warning when commits are already on the remote and an Undo.

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
cargo test --release perf_ -- --ignored --nocapture   # status and staging on 100,000 files
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

GitHub/GitLab integrations, submodules and worktrees, file editing.
