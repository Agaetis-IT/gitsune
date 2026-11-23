# git-hydra — Specification and Plan

A personal desktop git client. v1 is read-only and centered on a "metro map" view of the commit graph.

## 1. Goals and non-goals

**Goals (v1)**
- Open one or more local repositories and browse their history.
- Render the commit graph as a metro map: one colored line per branch, commits as stations.
- Show commit details (message, author, date, changed files, diff).
- Stay smooth on repositories up to ~100,000 commits.
- macOS first; Windows/Linux later (Tauri makes this possible, no work now).

**Non-goals (v1)** (version 2 adds editing: see section 12)
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
- Lane color: a name asks for the color given by its FNV-1a hash (remote prefix removed, so `main` and `origin/main` match) modulo the 12-color palette, and keeps it unless a name shown at the same time (overlapping rows) already took it; then it gets the next free color. When more names overlap than there are colors, the least used one is taken. Names are handled in hash order, so the result does not depend on the order of the lanes. Anonymous lanes have no color. (M9: a plain hash gave two simultaneous branches the same color, `main` included.)
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

### 5.5.1 Commit details and file diffs (M6)

- `get_commit_detail(repoId, id)` returns `{ id, parents, authorName, authorEmail, authorTime, message, files, filesTruncated }`. `files` are the changes against the **first parent** (against nothing for a root commit), sorted by path, with a status (`added`, `modified`, `deleted`, `renamed`, `copied`, `typeChanged`), the previous path for renames, and added/removed line counts. Renames are detected by `gix` with the repository's diff settings. Counts are skipped (`null`) for binary files, for blobs over 1 MiB, and when a commit lists more than 300 files; at most 5,000 files are listed.
- `get_file_diff(repoId, id, path, oldPath, force)` returns hunks of lines (`context`, `add`, `delete`, with old and new line numbers, `noEol` when the file has no final newline, 3 lines of context, `@@ -a,b +c,d @@` headers), computed by Rust with the histogram algorithm. Only the two blobs of that file are read, so it stays fast on huge commits.
- Limits: a blob over 1 MiB is not diffed unless `force` is set (`tooLarge`, `canForce`); over 8 MiB it is never diffed; a diff is cut at 5,000 lines (`truncated`, totals still exact); binary files (a NUL byte in the first 8,000 bytes, like git) and submodules have no hunks.
- The frontend shows the diff **in place of the graph** (the graph stays underneath with its scroll and selection; Escape or the Graph button comes back), in unified or side-by-side layout (long lines wrap in the latter). Lines are highlighted one by one with highlight.js, loading each language the first time it is needed; diffs over 3,000 lines are shown without highlighting. Unified and side-by-side rows are plain DOM tables, not virtualised: 5,000 lines is the practical ceiling.
- Selecting another commit clears the details and closes the open diff.

### 5.4.1 Search, highlight and branch hiding (M7)

- `search_commits(repoId, query, limit)` returns `{ rows, total }`: the rows (newest first, at most 2,000) of the **current graph** whose message (first 2,000 bytes, so the body too) or author contains **every** whitespace-separated word of the query, case-insensitively, or whose id starts with it. Because it searches the current graph, hidden refs are naturally excluded. The index (lowercased message, author and id of every commit) is built from the commits the first time a search is made and kept with the graph. On the 100,000-commit synthetic repository: index 273 ms once, then 3 to 5 ms per query.
- The search bar (top of the graph, Ctrl/Cmd+F) searches 200 ms after typing, jumps to the first match (centered and selected), tints every match row, shows `n of total`, and Enter / Shift+Enter or the arrows step through the matches (wrapping). Esc clears. The same words are searched again when the graph is rebuilt, without jumping.
- **Highlight**: a lane is drawn in full while the other lanes, their stations and the text of their commits fade. Clicking a lane line (not a station) opens a small menu with *Highlight this branch* and *Hide this branch*; the sidebar has a highlight button on each ref (refs now carry the `lane` of their commit), and a chip above the graph removes the highlight. Only lanes are highlighted, not the history below their fork point.
- **Hide**: hiding the branch of a lane hides its local branch **and** its remote-tracking branches (`main`, `origin/main`, ...) through the same hidden-refs mechanism as the sidebar. Lanes that are not named after a ref (merged branches whose ref is gone) can only be highlighted.

### 5.3 Live refresh (M8)

- **Watching.** When a repository is opened, the backend watches its `HEAD`, `packed-refs` and `refs/` (not `objects/`, the index or logs) with the `notify` crate. Changes are grouped over 300 ms, then the refs and HEAD are read and compared with the previous state: only a real change sends the `repo-changed` event `{ repoId }`. A `git add`, a `git gc` that packs refs, or a fetch with nothing new send nothing. The watcher lives as long as the repository is open; if it cannot start, the repository still works without refreshing by itself.
- **Rebuild, not patch.** The graph is rebuilt in full, but only the commits that appeared since the last build are read: the repository keeps what it already read (parents, time, merge subject; and the search entries). The result is exactly what a fresh open gives, because commits are put in a canonical order (by id) before the topological sort, so equal timestamps always tie the same way. Patching the layout of the old rows was rejected: a commit on top shifts every row and can change lanes, and a patch would have to reproduce the full algorithm to stay consistent. Measured on the 100,000-commit synthetic repository after one new commit: **64 to 75 ms with 22 refs**, 113 ms with the 1,199 refs of the stress repository (against 500 to 620 ms for the first build); the search index follows in about 6 ms.
- **Keeping the place.** `get_graph_meta` returns the new graph; `locate_commits(repoId, ids)` returns the rows of the selected commit and of the commit at the top of the view. The frontend then:
  - keeps that top commit at the same place on screen (unless the view was at the very top, which then shows the newest commits);
  - keeps the selection, the commit details, the open diff and the search (the same words are searched again, without jumping), and the highlighted branch (found again by name);
  - refreshes the sidebar refs and keeps the hidden refs hidden;
  - lets go of the selection if its commit is no longer in the graph.
- **No flicker.** While the new rows load, the previous picture stays on screen; it is replaced when the visible rows have arrived (or after 1.5 s at most).
- Changes arriving during a refresh are grouped into one more refresh; an answer that is no longer the latest is dropped.
- In a plain browser during development, `window.__simulateRepoChange()` in the console adds a commit on top of the demo repository and announces it like the watcher does.

### 5.4 Search
Search runs in Rust over the commit index (message, author, hash prefix). Results return oids; the frontend asks for the page containing the target and scrolls to it.

### 5.5 Diff
Computed in Rust, sent as structured hunks. The Vue component renders unified and side-by-side modes with light syntax highlighting. Large or binary files are collapsed with a "load anyway" action.

## 6. Performance targets and results

Measured at M9 on an Apple Silicon Mac, release build, on the synthetic repository (100,000 commits, 1,199 refs, ~4,000 lanes, 1,539 columns: a deliberately extreme shape) unless said otherwise.

| Target | Result |
|---|---|
| Open a 100k-commit repository and show the first screen in under 1 s | Graph built in 0.4 to 0.6 s (cold: reading the 1,199 loose refs is the slow part), then the first page in under 1 ms |
| Scrolling at 60 fps (only visible rows drawn) | Drawing one frame costs about 0.2 ms of CPU (95th percentile 0.3 ms) at 1000x800 and 2x pixel density, in compact and detailed modes, also with a highlight. Measured on the 500-commit demo graph; the cost depends on the rows on screen and on the number of lanes, not on the history length. The GPU side and a real 100k graph in the UI were not measured |
| Refresh after a new commit under 100 ms | 64 to 75 ms with 22 refs; 89 to 91 ms with the 1,199 refs of the stress repository (113 ms before the collision-free colors cost ~19 ms of layout there) |
| Bounded memory in the frontend | At most 30 chunks of 200 rows are kept (6,000 rows) |
| Search | 3 to 5 ms per query once the index exists (273 ms to build it, once) |

Known ceilings: a diff is a plain table, so about 5,000 lines is the practical limit; the layout colors cost grows with the square of the number of distinct branch names (negligible below a few hundred, ~19 ms at 4,000).

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

Status: M0 to M9 are done (see the sections above for what each one settled). Version 2: M10 (section 12.8), M11 (section 12.9) and M12 (section 12.10) and M13 (section 12.11) are done; M14 to M16 are planned (section 12.4).

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

## 10. Open questions

- None left from the plan. Ideas not done, in no particular order: panes that can be resized, restoring the open tabs at launch, a keyboard shortcut to open a repository, virtualising very long diffs, symbolic (non-hash) colors a user can pick per branch.

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

## 12. Version 2 — editing (plan)

Version 1 (M0 to M9) is read-only. Version 2 makes git-hydra a **daily-driver git client**: it writes to repositories. Everything here was decided in a question-and-answer session; each decision lists what was chosen. Items marked *proposed* are values or details not discussed, to be settled when the milestone starts.

### 12.1 Scope

- **Ambition**: a main client for everyday work, not a complement to the terminal. Stage, commit, branches, checkout, tags, fetch/pull/push, stash, cherry-pick, revert, reset, merge with conflict resolution, rebase (interactive included).
- **"Editing" means git writes plus an editor for conflicts.** There is no general file editor: code is still written in Zed.
- **Platform**: macOS only for now. Windows and Linux become a milestone of their own later (paths, inotify limits, askpass, signing). Code should still avoid needless macOS assumptions.
- **Out of scope for now**: GitHub, GitLab and pull requests (decision postponed until after the rebase milestone, so that no tokens need storing), submodules and worktrees, drag-and-drop of branches, staging of single lines (files and hunks only), `Signed-off-by` and `Co-authored-by` helpers.

### 12.2 Decisions

**How to write.** Every write goes through the **system `git` binary** (same behaviour as the terminal: hooks, config, signing, credentials). `gix` stays for reading the history (decision D1 still holds). The working tree status comes from `git status --porcelain=v2 -z`, not from `gix`.

**Keeping the status up to date.** The app watches the working tree (recursive, FSEvents on macOS) with a 300 ms debounce and runs one `git status` per burst. Events under ignored paths are discarded using `git check-ignore` with a cache, so a build writing to `target/` or `node_modules/` does not trigger endless status runs. Background reads use `GIT_OPTIONAL_LOCKS=0` so they never take the index lock away from the user's own git commands. The existing refs watcher (M8) and the `repo-changed` event are kept; the working tree gets its own event.

**Safety.**
- Destructive actions (delete a branch, `reset --hard`, discard changes, force push) ask for an explicit confirmation that says what will be lost. A force push is a separate, visible option, never the default, and uses `--force-with-lease`.
- Where possible the app offers **Undo**: for commits, resets, checkouts and branch deletions from the reflog or the previous tip; for **discarding working tree changes**, which the reflog cannot restore, a snapshot is taken first (`git stash create`, kept as an orphan commit under `refs/git-hydra/undo/`, the most recent 20 kept: *proposed*), and Undo restores it.
- A **per-repository read-only mode** disables every write (menus are greyed out and say why).

**Concurrency with the user's terminal.** Writes by the app are serialised in a queue per repository. When git reports a lock held by another process (`index.lock`), the app shows a clear message and offers to retry; **it never deletes lock files**.

**Transparency.** An **activity log** lists every git command the app ran (arguments, duration, output, error), copyable. **Long operations** (fetch, pull, push, rebase) stream progress from `--progress` and can be cancelled, which stops the git process cleanly.

**Remote authentication.** The app inherits the system configuration (credential helpers such as the macOS Keychain, `ssh-agent`). It runs git with `GIT_TERMINAL_PROMPT=0`; when git needs a password or a passphrase, a small helper set as `GIT_ASKPASS` / `SSH_ASKPASS` asks through a dialog of the app. Nothing is stored by the app.

**Actions on the graph.** Right-click menus on a commit, a branch label or a lane (the lane menu from M7 is extended), listing the actions valid in the current state; a greyed action explains why.

**Checkout with local changes.** git decides first (compatible changes follow the user). If git refuses, a dialog lists the files in the way and offers: stash and switch (then re-apply), discard (with a snapshot), or cancel.

**Commits (first milestone).** Summary and body (with a length counter for the summary), amend (pre-fills the previous message), hooks run as in the terminal and their output is shown when they fail, signing follows `commit.gpgsign` / the configured format, and a missing `user.name` / `user.email` is detected and offered to be set (globally or for the repository) instead of failing.

**Staging.** Files and hunks, by applying patches to the index (`git apply --cached`). Lines come later.

**Conflicts.** A **three-pane view** in the app: ours, theirs and the result, a choice per block, manual editing of the result, then mark as resolved. Text files only; for binary files the user picks a side.

**Interactive rebase.** The todo list (pick, reword, edit, squash, fixup, drop, reorder) is edited in the app. git runs with `GIT_SEQUENCE_EDITOR` pointing to a helper that injects that list. Stops (conflict, reword, edit) are handled by the operation-in-progress state below.

**Operation in progress.** The app detects merges, rebases, cherry-picks and reverts that are underway from the git directory (`MERGE_HEAD`, `rebase-merge/`, `rebase-apply/`, `CHERRY_PICK_HEAD`, `REVERT_HEAD`), also when started in a terminal, and shows a banner with continue, skip and abort. This state survives restarting the app.

### 12.3 Architecture

- **Runner** (`src-tauri/src/git/cli.rs`, new): one place that starts `git` with a controlled environment (the executable, `LC_ALL=C` for parsable messages, the askpass variables, `GIT_OPTIONAL_LOCKS=0` for reads), captures output, streams progress, supports cancellation, and writes an entry to the activity log. Nothing else spawns git for writes.
- **Operations**: one function per action (stage, unstage, commit, create branch...) returning a structured result (success data, or an error kind such as `Locked`, `HookFailed`, `NeedsIdentity`, `WouldOverwriteChanges`, `Conflict`) rather than raw text. The frontend maps error kinds to dialogs.
- **Queue**: a per-repository queue in the registry; reads (graph, status) do not wait for writes, but a write waits for the previous write.
- **After a write**: the refs watcher and the working tree watcher fire as for any change made outside the app, so the graph and the status refresh through the M8 path (rebuild, keep the place). The app does not patch its own state by hand.
- **Frontend**: a working tree row (WIP) at the top of the graph; selecting it replaces the commit details with the staged and unstaged lists and the message box, and diffs reuse the M6 view. The demo backend for browser development simulates writes in memory so the UI can be built without Tauri.

### 12.4 Milestones

Each milestone is a branch from `main`, ends with lint and tests passing, a demo, and a spec update, as in version 1.

| # | Content | Done when |
|---|---|---|
| **M10** | Runner, activity log, read-only mode, per-repository queue; working tree status and watcher; WIP row; stage and unstage files and hunks; commit and amend with hooks, signing and identity handling; discard with snapshot and Undo | A repository can be committed to from the app; hooks and signing behave as in the terminal; a lock held by another git process is reported, never broken |
| **M11** | Branches (create, rename, delete, set upstream), checkout with the dirty-tree dialog, tags (lightweight and annotated, delete), context menus on the graph | Switching branches never loses changes; deleting an unmerged branch warns and can be undone |
| **M12** | fetch, pull (merge or rebase), push (`--force-with-lease` as a separate option), askpass dialog, progress and cancel | Works against a local bare repository and an SSH or HTTPS remote through the user's credentials |
| **M13** | Stash (create, apply, pop, drop, with a list), cherry-pick, revert, reset (soft, mixed, hard) with Undo | Every destructive path has a confirmation and an Undo |
| **M14** | Operation-in-progress banner; merge (fast-forward, no-fast-forward, squash); conflict list and the three-pane resolver; abort | A conflicted merge can be resolved and committed without the terminal |
| **M15** | Rebase onto a branch; interactive rebase editor; continue, skip, abort; conflicts through the M14 resolver | A rebase with a conflict, a squash and a reword completes from the app |
| **M16** | Polish: performance of status on large working trees, error and empty states of the new flows, packaging, documentation | Targets of section 12.6 met |

### 12.5 Testing

- Every write operation is tested on **throwaway repositories made with the git CLI** (invented data only), and its result is compared with what git itself gives (same commit tree, same refs, same index).
- Tests also cover the failure paths: a hook that fails, a missing identity, a lock held by another process, a checkout that would overwrite changes, a conflict, a cancelled fetch, an Undo after each destructive action.
- No test touches a real repository of the developer, and none reaches the network (remotes are local bare repositories).
- The working tree watcher is tested for noise: ignored directories written to continuously must not trigger status runs.
- The frontend logic (queue states, dialogs by error kind, WIP selection, the todo list editor, the three-pane resolver's merge logic) is tested with a fake backend, as in version 1.

### 12.6 Targets (*proposed*)

- `git status` refresh after a change in a 100k-file working tree under 300 ms on this machine, and never more than one status running at a time.
- Opening the WIP row and showing the staged and unstaged lists in under 100 ms once the status is known.
- Staging or unstaging a file reflected on screen in under 150 ms.

### 12.7 Risks and open points

- **Interactive tools**: hooks or commands that expect a terminal (prompts, pagers) cannot be driven; the runner closes standard input, and such a hook fails with its output shown.
- **Signing**: GPG and SSH signing may ask for a passphrase through `pinentry` or an agent; behaviour with the askpass helper must be checked in M10.
- **Large working trees**: status can be slow without the untracked cache or `core.fsmonitor`; M16 measures it and may suggest enabling them rather than doing it silently.
- **Special setups** not designed for yet: sparse checkout, Git LFS filters, submodules, linked worktrees.
- **Undo snapshots** add refs to the user's repository (`refs/git-hydra/undo/`); the app lists them, purges old ones, and says so in the documentation.
- **Postponed**: GitHub and GitLab, Windows and Linux, line-level staging, drag-and-drop, sign-off and co-author helpers, resizable panes and other ideas of section 10.

### 12.8 M10 as built (staging and commits)

What exists, and where it differs from the plan above.

**Runner and safety** (`src-tauri/src/git/cli.rs`, `activity.rs`, `store.rs`)
- Every write starts `git` through one runner: `LC_ALL=C`, `GIT_TERMINAL_PROMPT=0`, `GIT_PAGER=cat`, `GIT_EDITOR=true`, standard input closed unless a payload is given (a hook that waits for input fails instead of hanging). Background reads set `GIT_OPTIONAL_LOCKS=0`.
- Failures are typed: `readOnly`, `locked`, `needsIdentity`, `hookFailed`, `signingFailed`, `nothingToCommit`, `staleHunk`, `other`. The frontend maps them: hook, signing and nothing-to-commit are shown in the commit box with git's output; a missing identity opens a dialog; the others are notices.
- A repository's writes are serialised by a lock taken before git starts; read-only mode is checked before and after waiting for it, so a write queued behind another cannot slip through when the mode is switched on. Read-only mode is remembered per repository in the webview storage and sent to the backend when the repository opens.
- A lock held by another git process (`index.lock`) is reported as `locked` and the file is never touched (tested).
- **Activity log**: at most 500 entries per repository (command, start, duration, exit code, output up to 16 KiB), shown in a side panel. The status reads the app does in the background are deliberately **not** logged, otherwise every file save would fill it; every command that changes something is.

**Status** (`status.rs`): `git status --porcelain=v2 --branch -z --untracked-files=normal`, parsed into staged, unstaged (untracked files included, an untracked folder is one entry) and conflicted lists, plus branch, HEAD, upstream and ahead/behind. Conflicts are listed but cannot be resolved yet (M14).

**Staging** (`ops.rs`, `text_diff.rs`)
- Files: `git add -A -- paths` and `git restore --staged -- paths` (`git rm --cached` before the first commit), in chunks of 400 paths.
- Hunks: every diff hunk now has an `id` (a hash of its position and exact bytes). Staging a hunk recomputes the diff in Rust, finds the hunk with that id (otherwise the error is `staleHunk` and nothing is staged), builds its patch **from the raw bytes** (so CRLF files, missing final newlines and non-UTF-8 content survive, and odd paths are C-quoted) and runs `git apply --cached --recount -` (with `--reverse` to unstage). Hunks of untracked, deleted and binary files are refused with a reason; those are staged as a whole.
- Working tree diffs (`working_diff`) reuse the commit diff limits (1 MiB, 8 MiB forced, 5,000 lines).

**Commits**: `git commit -F -` with the summary, a blank line and the description; `--amend` for amending (the box is pre-filled with the last message, and the previous draft is put back when "amend" is unchecked). Hooks and signing are git's own; the test for signing pins `gpg.format=openpgp` because a global configuration may use SSH signing. Identity: `identity` / `set_identity` (`--global` or `--local`); the commit is retried after the dialog. Undo of a commit moves HEAD back with `git reset --soft` (or `update-ref -d HEAD` for a first commit) and is refused when HEAD is no longer the commit that was made.

**Discarding and recovering**: before changes are discarded, their working tree content (untracked files and deletions included) is saved as a commit under `refs/git-hydra/undo/<millis>-<counter>`, built with a throwaway index file so the real index, HEAD and working tree are untouched, using a fixed `git-hydra` identity so it works without configuration. The 20 newest are kept. Restoring a snapshot first snapshots what it overwrites, so a restore can itself be undone. Tracked changes go back with `git restore`, untracked files and folders are deleted with `git clean -f -d`; staged changes are kept.

**Working tree watcher** (`worktree_watch.rs`): a recursive watch of the working tree with a 300 ms debounce. Events inside `.git` count only for `index` and `HEAD`; events for other paths are filtered by a single `git check-ignore -z --stdin` per burst (so a build writing into `target/` or `node_modules/` triggers nothing, and `.gitignore` edits take effect at once, since no ignore cache is kept). It sends `worktree-changed`; the existing refs watcher still sends `repo-changed`. Activity sends `activity-changed`.

**Interface**: a working tree row pinned above the graph (a dashed station on the lane of HEAD, "n changes"); selecting it, or opening a repository without commits, replaces the commit details with the working panel (unstaged and staged lists, per-file stage, unstage and discard, "recently discarded", the commit box with a summary counter at 72 characters, a soft convention, and Cmd/Ctrl+Enter). Diffs of working tree files open in place of the graph with a *Stage hunk* / *Unstage hunk* button per hunk and reload quietly when the status changes. Notices with an *Undo* button follow commits and discards. The activity panel and the read-only switch are in the toolbar.

**Demo backend**: `src/dev/demoWorktree.ts` simulates all of this in memory for browser development (magic words in a summary trigger the failures: `hook`, `sign`, `ident`); a commit made there adds a commit to the demo graph. `window.__simulateWorktreeChange()` adds a file as if saved from an editor.

**Not done in M10 (left for later milestones or ideas)**: discarding a single hunk; `Signed-off-by` and co-author helpers (declined for this milestone); per-file Undo of a stage (the file can simply be unstaged); resolving conflicts (M14); measuring `git status` on a 100,000-file working tree (M16); progress and cancellation of long operations (M12, there is no long operation yet).

**Tests**: 132 Rust tests (status parsing on sample output and on real repositories; staging by file and by hunk including CRLF, no final newline, latin-1 bytes, spaces, quotes and accents in paths, stale hunks, binary and untracked refusals; commits, amend, undo, hooks that reject or wait for input, missing identity, failing signature, lock, read-only; discard, snapshot, restore, purge, restore of a restore; ordering of writes; the working tree watcher with ignored folders, `.gitignore` changes, staging, commits and git internals) and 114 front tests (status refresh and coalescing, selection exclusion, stage and hunk flows, read-only persistence, commit success and each failure, identity retry, amend, discard Undo, activity reload, notices). A mutation of the patch builder (dropping the "no newline" marker) is caught by the tests.

### 12.9 M11 as built (branches, checkout, tags)
What exists, and where it differs from the plan above.

**Backend** (`src-tauri/src/git/branches.rs`): every operation goes through the same runner and write lock as M10, and is listed in the activity log.
- Names are validated with `git check-ref-format` before anything runs (`invalidName`); a name already used is `alreadyExists`.
- **Create branch** (`git branch`, or `git switch -c` when the new branch is checked out) from a commit or from HEAD. **Rename** (`git branch -m`, local only). **Set upstream** (`git branch --set-upstream-to`, or `--unset-upstream`).
- **Checkout** takes a target: local branch, remote-tracking branch (creates the local branch, or switches to it if it exists), commit or tag (detached HEAD). Without a strategy git decides: changes that would be overwritten fail as `wouldOverwriteChanges` with the list of files, and nothing has been touched. With `stash`: `git stash push --include-untracked`, switch, `git stash pop --index`; if the pop conflicts, the stash is kept and the result says so. With `discard`: a snapshot (M10) is taken first, then `git switch --discard-changes`; the result carries the snapshot so the notice can offer *Undo*.
- **Delete branch** uses `git branch -d`; an unmerged branch fails as `notFullyMerged` with git's message, and `force` uses `-D`. A branch checked out here or in another worktree is `branchCheckedOut`. The result holds the tip and upstream, so *Undo* recreates the branch (`restore_branch`).
- **Tags**: lightweight, or annotated when a message is given (the annotated tag needs an identity; a bookkeeping identity is used when none is set so it never prompts). Deleting returns what the tag pointed at, so *Undo* restores it (`restore_tag`).

**Interface**: right click on a pill in the graph, on a ref in the sidebar, on a commit or on a lane line opens a context menu (`ContextMenu.vue`, items built by `src/lib/menus.ts`; items that cannot apply are greyed with the reason, and every write item says "Read-only mode is on." in that mode). Dialogs (`BranchDialogs.vue`): create branch (with "switch to it"), rename, delete (second step for an unmerged branch, explaining that *Undo* brings it back), create tag (annotated option), delete tag, upstream picker, and the dirty-tree dialog (*Stash and switch*, *Discard and switch*, *Cancel*). Name problems are shown inside the dialog; other failures are notices.

**Demo backend**: the stand-in backend simulates branches, tags and checkout in memory (the demo working tree counts as dirty, so the first switch shows the dirty-tree dialog).

**Not done in M11**: deleting a remote branch (needs M12), a "delete branch and its remote" option, checkout of a branch checked out in another worktree (reported as an error).

**Tests**: 153 Rust tests (21 new: error classification, creation, each checkout target, dirty trees with stash and discard, rename, delete and restore, upstream against a local bare remote, tags with and without identity, read-only mode, lock) and 139 front tests (checkout flows and notices, dialogs' errors, delete with unmerged warning, Undo, menu builders).

### 12.10 M12 as built (fetch, pull, push)

**Backend** (`src-tauri/src/git/remote.rs`, runner additions in `cli.rs`).
- `Runner::run_long` runs git with its stderr streamed line by line (git's `\r` progress updates go to the interface, only the lines git ended with `\n` are kept for the log), can be cancelled, and relays questions. It starts git in its own process group: cancelling sends `SIGTERM` to the group (git removes its lock files, `ssh` and hooks stop too) and `SIGKILL` after 3 seconds.
- **Askpass**: a small shell script written to a per-operation temp directory is set as `GIT_ASKPASS` and `SSH_ASKPASS` (`SSH_ASKPASS_REQUIRE=force`). It writes the question to `<pid>.prompt` and waits for `<pid>.answer` (or `<pid>.cancel`); the runner reports new questions through a callback and `answer_prompt` writes the answer. No extra binary is shipped; the answer only passes through a file that lives for the duration of the operation and is deleted with the directory. Nothing is stored.
- Operations: `fetch` (all remotes or one, optional `--prune`), `pull` (git's own configuration, or `--ff-only`, `--no-rebase`, `--rebase`), `push` (upstream, or remote and branch; `-u`; `--force-with-lease` as a separate option, never `--force`), `delete_remote_branch` (`push <remote> --delete <branch>`), `list_remotes`. Names that look like options are refused before git runs.
- **Fetch is allowed in read-only mode** and does not take the write lock (it only moves remote-tracking refs); pull, push and remote deletion are writes.
- New error kinds: `authFailed`, `unreachable`, `pushRejected`, `noUpstream`, `divergedBranches`, `conflict`, `cancelled` (and `would be overwritten by merge` is a `wouldOverwriteChanges` with its files).
- Commands: `fetch_remote`, `pull_branch`, `push_branch`, `delete_remote_branch`, `list_remotes`, `cancel_operation`, `answer_askpass`; events `remote-progress` and `askpass-request`. The interface chooses the operation id. The operation runs off the async runtime.

**Interface.** Fetch, Pull and Push buttons in the toolbar; a progress panel (percentage when git gives one, last line, Cancel); an askpass dialog (password or passphrase hidden, host-key questions in clear). Failures lead to the right question instead of an error: no upstream opens the push dialog with "track it" ticked; a push refused because the remote is ahead opens it with "Pull first" and the lease option; a pull on diverged branches asks Merge or Rebase; a pull that conflicts or would overwrite local changes is explained in a notice. Context menus: "Push…" on a local branch, "Delete on the remote…" on a remote branch.

**Demo backend** (`src/dev/demoRemote.ts`) plays progress, asks one password the first time something is pushed, needs an upstream for a first push and a strategy for the first pull.

**Not done in M12**: ahead/behind counters on branches, tags push (`--tags`), adding or removing remotes, a "pull" that stashes local changes automatically, conflict resolution after a pull (M14: the merge is left in progress and explained), reading the credential helper's result for HTTPS (git does it itself). The askpass flow was tested with `git credential fill`; no test reaches a real SSH or HTTPS server, and the Tauri window itself was not run.

**Tests**: 173 Rust (20 new: classification, fetch with progress and prune, read-only, pull fast-forward/merge/rebase/conflict/overwrite, push with upstream, lease, deletion, option-looking names, cancelling a hung fetch, askpass answers and refusal) and 152 front.

### 12.11 M13 as built (stash, cherry-pick, revert, reset)

**Backend** (`src-tauri/src/git/stash.rs`, `rewrite.rs`).
- **Stash**: `list_stashes` (position, commit id, message, time), `stash_push` (message, `--include-untracked`, `--keep-index`; "nothing to stash" is an error, not a silent success), `stash_apply`, `stash_pop`, `stash_drop`, `stash_restore` (`git stash store`, which brings a dropped stash back: its commit is still in the repository). Every action takes the position **and the commit id** the interface saw: if the list changed meanwhile (a terminal dropped one) it is refused instead of acting on another stash. A pop or apply that conflicts keeps the stash and is reported as `conflict`.
- **Cherry-pick and revert** of one commit onto HEAD. A merge commit follows its first parent (`-m 1`). Nothing is left half-way: a conflict, or a commit whose changes are already in the branch (`nothingToCommit`), cancels the operation with `--abort`; the app refuses to start if a merge, cherry-pick or revert is already waiting (it never aborts one it did not start). Local changes in the way are reported with their files. **Undo** puts HEAD back with `git reset --keep`, which refuses if HEAD moved since or if it would lose local changes.
- **Reset** soft, mixed or hard to a commit. A hard reset first takes a snapshot of the tracked changes (same mechanism as discard); **Undo** moves the branch back (`--keep`, refused if HEAD moved) and restores the snapshot. After a mixed reset Undo restores the branch but not what had been staged (the content is kept, unstaged).
- Conflict detection now comes before "nothing to commit" in the message classifier, because git prints both for a conflicted stash.
- Commands: `list_stashes`, `stash_push`, `stash_apply`, `stash_pop`, `stash_drop`, `stash_restore`, `cherry_pick`, `revert_commit`, `undo_replay`, `reset_to`, `undo_reset`.

**Interface.** A Stash button (message, include new files, keep what is staged) and a Stashes section in the sidebar (apply, pop, drop with a confirmation and Undo). The commit context menu gains "Cherry-pick onto the current branch" (greyed on the current commit), "Revert this commit" and "Reset the current branch to here…", whose dialog explains the three modes and warns how many files a hard reset would discard. Every action greys out in read-only mode and ends with an Undo notice.

**Demo backend** (`src/dev/demoStash.ts`): an in-memory stash list, and canned answers for the others (cherry-picking a commit whose id starts with `c0` conflicts).

**Not done in M13**: cherry-picking several commits at once, reverting without committing, stash of a single file, applying a stash with `--index` from the interface (the backend supports it), a diff view of a stash, branch from a stash, resolving the conflicts these operations can raise (M14), an in-app way out of a merge left half-done by a pull (M14). Tests: 196 Rust (23 new) and 164 front. Not run: the Tauri window.

