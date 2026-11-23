# git-hydra — Specification and Plan

A personal desktop git client. v1 is read-only and centered on a "metro map" view of the commit graph.

## 1. Goals and non-goals

**Goals (v1)**
- Open one or more local repositories and browse their history.
- Render the commit graph as a metro map: one colored line per branch, commits as stations.
- Show commit details (message, author, date, changed files, diff).
- Stay smooth on repositories up to ~100,000 commits.
- macOS first; Windows/Linux later (Tauri makes this possible, no work now).

**Non-goals (v1)**
- Any git write operation (stage, commit, push, pull, merge, rebase, checkout).
- GitHub/GitLab/other integrations.
- Conflict resolution.
- Submodules and worktrees.
- File editing.

## 2. Stack

| Layer | Choice |
|---|---|
| Shell | Tauri 2 |
| Backend | Rust |
| Frontend | Vue 3 + TypeScript + Tailwind + shadcn-vue |
| Package manager / runner | Bun |
| Git read access | `gix` or `git2` (decided during the prototype, see M1) |
| Git write access (later) | `git` CLI (keeps hooks, SSH, credentials behaviour) |
| FS watching | `notify` crate |
| Tests | Rust (`cargo test`) + Vitest |

## 3. UI layout

Three zones, per repository tab:

1. **Left sidebar** — local branches, remote branches, tags (collapsible groups, visibility toggles).
2. **Center** — metro graph (Canvas) with a commit list aligned to the stations.
3. **Right panel** — details of the selected commit: message, author, date, hash, parents, changed files, diff.

Global: tabs per repository, recent repositories list, light/dark theme following the system.

## 4. Metro graph

### 4.1 Visual identity
Clean transit-map style, inspired by GitKraken but not copying it:
- Thick lines, orthogonal paths with rounded corners at forks and merges.
- Stations are white dots with a colored ring; merge commits get a distinct marker.
- Branch labels placed at the line heads, like line names on a transit map.
- Saturated but harmonious palette, tested in light and dark themes.

### 4.2 Lane rules
- One lane per named branch. Color is stable and derived from the branch name (hash into the palette).
- The main line follows the **first parent**.
- A merged branch whose ref no longer exists keeps its own lane, named from the merge commit message (e.g. "Merge branch 'x'"); if no name can be derived, it becomes an anonymous lane.
- Commits not reachable from a named branch go on anonymous lanes (neutral color).
- Lane assignment is deterministic: same repo state produces the same layout.

### 4.3 Interactions (v1)
- Scroll and select a commit (updates the right panel).
- Hover a station: tooltip with hash, message, author, date.
- Filter/hide branches: click a lane or sidebar entry to hide or highlight it.
- Search commits by message, author or hash, with jump to the result.
- Zoom: switch between compact and detailed row density.

### 4.4 Refs shown by default
Local branches, remote branches and tags. Remotes and tags can be hidden through filters.

## 5. Architecture

### 5.1 Responsibilities
- **Rust** owns all git access, the lane layout algorithm and diff computation. The frontend never parses git data.
- **Vue** owns the UI shell, panels, sidebar, search box and diff rendering.
- **Canvas 2D** owns graph drawing only; it receives pre-computed geometry.

### 5.2 Data flow
1. Frontend asks for a page of commits: `get_graph_page(repo_id, cursor, limit)`.
2. Rust walks commits in topological order, assigns lanes incrementally, and returns rows:
   `{ oid, parents, lane, lane_color, edges: [...], refs: [...], summary, author, time }`.
3. The frontend keeps only the visible window plus a margin and requests further pages while scrolling (virtualization).
4. Selecting a commit calls `get_commit_detail(repo_id, oid)`; file diffs are loaded per file through `get_file_diff(repo_id, oid, path)` (hunks as JSON).

Lane state (active lanes at the cursor) is kept in Rust per repository so that paging stays consistent without recomputing the whole history.

### 5.3 Live refresh
A `notify` watcher on `.git` (HEAD, `refs/`, `packed-refs`) triggers an incremental update: only new commits and changed refs are recomputed, and the frontend receives a `repo-changed` event with the affected range. Events are debounced.

### 5.4 Search
Search runs in Rust over the commit index (message, author, hash prefix). Results return oids; the frontend asks for the page containing the target and scrolls to it.

### 5.5 Diff
Computed in Rust, sent as structured hunks. The Vue component renders unified and side-by-side modes with light syntax highlighting. Large or binary files are collapsed with a "load anyway" action.

## 6. Performance targets
- Open a 100k-commit repository and show the first screen in under 1 s (first page only).
- Scrolling stays at 60 fps (only visible rows drawn).
- Incremental refresh after a new commit under 100 ms.
- Memory: the frontend holds a bounded window of rows, not the whole history.

## 7. Testing strategy
- **Rust unit tests** for the lane algorithm on **synthetic repositories created by script** (linear history, fork/merge, octopus merge, deleted merged branches, many parallel branches). All data is invented; no real repositories in fixtures.
- **Rust integration tests** for paging, search and incremental refresh.
- **Vitest** for components and the diff renderer.
- **Performance check**: a script generates a 100k-commit synthetic repository, used for manual and benchmark runs.
- Lint (`clippy`, `rustfmt`, ESLint) and tests run before declaring any milestone done.

## 8. Project conventions
- Code, comments, commit messages in English.
- Bun for all JS tooling.
- Work on feature branches only; commits happen only when explicitly requested.

## 9. Plan

Each milestone ends with lint and tests passing and a short demo.

**M0 — Scaffold**
- `git init` on a branch (not `main`/`develop`), Tauri 2 + Vue 3 + TS + Tailwind + shadcn-vue with Bun.
- Lint/format config, CI-less local scripts, theme (light/dark) wiring.

**M1 — Git access prototype**
- Benchmark `gix` vs `git2` on a 100k-commit synthetic repo (commit walk, ref listing, diff). Pick one and record the decision.
- Script generating synthetic repositories (invented data).
- Tauri commands: open repository, list refs.

**M2 — Lane layout engine (Rust)**
- Topological walk, first-parent main line, lane allocation and reuse, edge geometry (fork/merge curves).
- Named lanes from refs, merge-message-derived names, anonymous lanes.
- Stable color assignment.
- Full unit-test coverage on the synthetic cases.

**M3 — Paged graph API**
- `get_graph_page` with cursor, persistent lane state, row model.
- Integration tests and the 100k benchmark.

**M4 — Metro canvas renderer**
- Canvas component with virtualized rows, line/station/label drawing in the metro style.
- Scrolling, selection, hover tooltip, density zoom.

**M5 — App shell**
- Three-zone layout, sidebar (branches/remotes/tags with visibility toggles), repository tabs, recent repositories, theme.

**M6 — Commit detail and diff**
- `get_commit_detail`, `get_file_diff`, unified and side-by-side rendering, large/binary handling.

**M7 — Filters and search**
- Branch hide/highlight (lane click and sidebar), commit search with jump-to-result.

**M8 — Live refresh**
- `notify` watcher, debounce, incremental recompute, `repo-changed` events, UI patching without losing scroll or selection.

**M9 — Polish**
- Visual tuning of the palette and metro style in both themes, empty/error states, performance pass against the targets in section 6, macOS packaging (unsigned build is enough for personal use).

## 10. Open questions (to settle during the milestones)
- `gix` vs `git2`: decided at M1 from the benchmark.
- Exact palette and line thickness: tuned at M4/M9.
- Whether to add a minimal commit list column (hash, author, date) next to the graph or keep only summaries: decide at M4.
