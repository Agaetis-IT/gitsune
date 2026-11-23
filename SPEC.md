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
| Git read access | `gix` (decided at M1, see section 11) |
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

### 4.2.1 Layout engine (M2, `src-tauri/src/layout`)

- Commits are ordered topologically (children before parents), newest first among the ready ones; this stays correct with clock skew.
- Every other parent (merge parents beyond the first) starts a new lane in the **nearest free column** (right side on ties), to minimise crossings. The tip of a new line takes the lowest free column.
- A lane **keeps its column for its whole life**. When several lanes lead to the same commit (a fork point), they all run down to it and bend into the leftmost lane in the last span; the leftmost lane carries the commit.
- Output per row: commit column, lane id, and only the **non-vertical connectors** (`Start` bend right after a merge commit, `End` bend right before the fork point). Vertical segments are implied by each lane's `col`, `firstRow` and `lastRow`, which keeps memory proportional to the number of bends rather than to rows x active lanes.
- Lane name: best ref on the lane (HEAD branch, then local, then remote, then tag; earliest row wins ties), else the branch parsed from the merge commit subject that started the lane (`Merge branch 'x'`, `Merge remote-tracking branch 'origin/x'`, `Merge pull request #n from owner/x`), else anonymous.
- Lane color: FNV-1a hash of the lane name (remote prefix removed, so `main` and `origin/main` match) modulo the 12-color palette. Anonymous lanes have no color.
- Measured on the 100,000-commit synthetic repository (1,199 refs): read and order the graph 145 ms, layout 13 ms. The synthetic repo keeps ~1,180 unmerged branches alive, which gives 1,539 columns; real repositories are far narrower, and very wide graphs are handled by branch hiding (M7).

### 4.2.2 Renderer (M4, `src/lib/metro`, `src/components/graph/MetroGraph.vue`)

- One Canvas 2D for the graph and the commit text, in a scroll container: a sticky canvas the size of the viewport and a spacer that gives the scrollbars their range. Only visible rows are drawn (virtualization); rows arrive by chunks of 200 through a bounded cache (30 chunks, 1 chunk prefetched on each side, stale responses ignored after a reset).
- The lane area takes at most 45% of the width and **scrolls sideways on its own**; the commit text stays fixed. This keeps very wide graphs usable until branch hiding (M7).
- Lanes are straight verticals (from `Lane.col`, `forked`, `joins`, `firstRow`, `lastRow`); only connectors come from row data. Corners are quadratic curves of radius 9 px (compact) / 12 px (detailed).
- Stations: ring in the lane color with the theme background inside; merge commits are bigger "interchange" stations with a dot in the middle.
- Commit list: ref labels (local filled in the lane color, HEAD outlined and bold, remotes outlined, tags grey), the summary, then author and date on the right in compact mode; in detailed mode a second line with author, date and short id. This settles the open question about a commit list column: it is drawn in the same canvas, aligned with the stations.
- Interactions: click and arrow/Page/Home/End keys select a row; hovering a station shows a tooltip (summary, short id, author, date, merge info); Ctrl/Cmd + wheel (or trackpad pinch) switches the density, keeping the same commit at the top.
- Theme: colors are read from the CSS variables of the app (`--background`, `--foreground`, `--muted-foreground`) and the 12-color palette has a light and a dark variant (`palette.ts`, must stay in sync with `PALETTE_SIZE` in Rust).
- In a plain browser during development (no Tauri) the app shows a recorded graph of an invented 500-commit repository (`src/dev/graph-demo.json`, generated from the Rust backend; see `dump_graph_fixture` in `src-tauri/src/git/tests.rs`). It is excluded from production builds.

### 4.2.3 App shell (M5)

- Tabs per repository (middle-click or the cross closes), a `+` button opening the system folder picker (`tauri-plugin-dialog`), and a welcome screen with the 8 most recently opened repositories (kept in the webview's local storage, best effort).
- A repository already open is activated instead of opened twice. Selection and scroll position are remembered per tab.
- Left sidebar: local branches, remotes and tags in collapsible groups (tags start collapsed), a visible/total counter, an eye per ref and per group, a `HEAD` badge, and a click on a ref jumps to its commit. Hidden refs are dimmed.
- Right panel: summary, full id (with copy), author, date, parents and refs of the selected commit. Changed files and diff come with M6.
- The theme button cycles system, light and dark.
- Application state lives in `src/composables/useWorkspace.ts`, built on a `RepoBackend` interface (`src/lib/backend.ts`) so it can be tested without Tauri; in a plain browser during development a recorded demo repository replaces the backend (hidden refs have no effect there).

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

### 5.2.1 Paged graph API (M3)

- `get_graph_meta(repoId)` reads and lays out the whole graph (replacing any previous layout) and returns `{ total, maxCols, lanes, refs, loadMs, layoutMs }`. Lanes are sent once; the frontend derives vertical segments from each lane's `col`, `firstRow` and `lastRow`.
- `get_graph_page(repoId, cursor, limit)` returns `{ start, rows, nextCursor, total }`. `cursor` is a row index, `limit` is clamped to 1..1000 and `nextCursor` is `null` at the end.
- A row carries: `row`, `id`, `parents` (ids), `summary`, `author`, `time`, `col`, `lane`, `edges` (non-vertical connectors to the next row) and `refs`. Commit details are read from git per page; only ids, layout and refs stay in memory.
- Every page is a slice of the same cached layout, so lanes and columns never change between pages. The cache is replaced by the next `get_graph_meta` call; incremental updates come with M8.
- Types for the frontend live in `src/lib/api.ts`.
- Measured on the 100,000-commit synthetic repository: full build 412 ms, meta 599 KiB (3,989 lanes, 1,199 refs), a 200-row page about 0.7 ms and 48 KiB, all 100 pages of 1,000 rows walked in 305 ms.

### 5.2.2 Hidden refs (M5)

- `get_graph_meta(repoId, hiddenRefs)` takes full ref names (`refs/heads/x`). The graph is built from the refs that are **not** hidden: commits only reachable from hidden refs disappear, and the layout is computed again. `meta.refs` lists the visible refs, each with the `row` of its commit (used by the sidebar to jump to a branch).
- Opening an already open repository keeps its cached graph.
- Per-ref and per-kind toggles in the sidebar both go through this call. Hiding by clicking a lane and highlighting stay in M7.

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
- Exact palette and line thickness: tuned at M4/M9.
- Lane colors come from a hash of the name modulo 12, so two branches can share a color (seen on the demo graph: `main` and a merged feature branch). Options if it bothers in practice: bigger palette, or assign colors first-come per repository while keeping them stable across sessions.

## 11. Decisions

### D1 — `gix` for git read access (M1)

Benchmark on a synthetic 100,000-commit repository (1,199 refs, ~4,000 merges, 47 MiB pack), release build, warm cache, Apple Silicon. Reproduce with `scripts/gen-synthetic-repo.ts` and `docs/benchmarks/bench_git.rs`.

| Operation | `git2` | `gix` |
|---|---|---|
| Open repository | 0.3 ms | 0.6 ms |
| List 1,199 refs | 34 ms | 40 ms |
| First 1,000 commits (with summary) | 594 ms | 3.5 ms |
| Full graph in one pass (id, parents, time) | 614 ms | 50 ms (32 ms with commit-graph) |
| Diff of one commit, file list | 0.9 ms (with patch text) | 0.3 ms (file list only) |

Why `gix`: the lane layout needs the whole parent graph once; `gix` reads 100k commits in ~50 ms against ~600 ms for `git2`, which keeps the first screen well under the 1 s target. It is also pure Rust (no C build of libgit2).

Caveats:
- The two walks are not strictly equivalent: `git2` was asked for a topological order, `gix` for commit-time order. The layout engine will compute its own topological order from the parent graph (needed anyway, and robust to clock skew), so this does not affect the choice. The "full graph" rows use the same sort (commit time) for both.
- `gix` hunk-level diffs were not benchmarked (only the file list). M6 will use `gix`'s blob diff; if it falls short, fall back to the `git` CLI for that path only.
