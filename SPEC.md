# git-hydra

A desktop git client built around a **metro-style branch graph**: one colored line per branch, commits as stations, merges as interchanges. Browsing history, searching and reading diffs are complete. Writing is in too (version 2): staging by file and hunk, committing and amending, discarding, branches, checkout, tags, fetch, pull, push, stash, cherry-pick, revert, reset, merge, conflict resolution and rebase (see [SPEC.md](SPEC.md), section 12). Every change the app makes goes through the `git` binary and is listed in an activity log; a read-only switch per repository refuses all writes.

Tauri 2 (Rust) + Vue 3 + TypeScript + Tailwind + shadcn-vue. macOS first.

## Features

- Metro graph on a virtualized canvas, comfortable up to ~100,000 commits. Each station shows the initials of the author on a color of their own (the ring keeps the color of the branch; it can be switched off in Settings).
- Tabs per repository, recent repositories, system light/dark theme.
- Sidebar with local branches, remotes and tags, in folders by the `/` of their names (`feature/auth-18` is in `feature`; a remote's name is its first folder): close and open folders (remembered per repository), hide a branch, a folder or a group, highlight a branch, jump to its commit.
- Click a lane in the graph to highlight or hide its branch.
- Search commits by message, author or id: the *Search* button or Ctrl/Cmd+F opens a small box at the top right of the graph (Enter and Shift+Enter step through the matches, Esc closes it).
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

## 17. Search box and density

The **search** is a *Search* button (a magnifier and the word) in the toolbar and Ctrl/Cmd+F; both open a small box floating at the top right of the graph, which closes with Esc or its close button and then clears the search; it closes when another repository is shown. The **compact / detailed** choice is gone from the toolbar: it lives in *Settings, Appearance, Graph* and is remembered (`git-hydra:graph-density`); Ctrl+wheel on the graph still changes it, and the choice is saved the same way.


## 18. Three-column graph

The graph has three columns under a header (*Branch / tag*, *Graph*, *Commit message*): the **labels** of the branches and tags on the left, the **lanes** in the middle, the commit message on the right. A label is linked to its station by a line in the color of the branch, and every row is faintly tinted in that color.

- **One label per branch.** `feature/x`, `origin/feature/x` and `upstream/feature/x` on one commit make a single label (`src/lib/metro/refGroups.ts`) with a laptop when the branch exists locally and one icon per remote that has it; a tag has a tag icon; the current branch has a check mark. Labels that do not fit are folded into `+N`. `origin/HEAD` is not shown.
- **Icons per remote.** Chosen by remote name in *Settings, Graph, Remote icons* from a fixed set (Lucide outlines drawn on the canvas, `src/lib/metro/icons.ts`); the default is people for `origin`, fork for `upstream`, server for the others. Stored in `git-hydra:remote-icons`.
- **Resizable columns.** The dividers in the header are dragged (or moved with the arrow keys; double click resets): `git-hydra:graph-refs-width` and `git-hydra:graph-width` (the lane area fits the graph when no width is set).
- **Optional columns** right of the message: author, date, commit id (`git-hydra:graph-columns`; author and date on by default). In the detailed density, what is not in a column stays under the message.

## 19. Session and menu bar

**Session.** The open tabs, the one shown and the refs each hid are saved on every change (`git-hydra:session`, `src/lib/session.ts`) and reopened in order at the next start (`restoreSession` in the workspace). A tab closed by hand is not brought back. A repository that cannot be opened any more is left out, with one notice naming it; it stays in *Recent*. Nothing is saved while the tabs are being reopened, so a partial state never replaces the session. *Settings, General, Restore the tabs at start* (on by default, `git-hydra:restore-tabs`) switches it off; the session is still saved so that switching it on later works. The scroll position and the selected commit are not kept: the graph goes to HEAD as for any opening. *Recent* keeps 10 repositories and can be cleared from the menu.

**Menu bar (macOS).** The default menu is replaced by one built from `buildMenuModel` (`src/lib/appMenu.ts`, pure and tested) and set with Tauri's JS menu API (`src/lib/nativeMenu.ts`), rebuilt whenever its state changes: git-hydra (About, Settings, Quit...), File (Open, Open Recent, Close the Tab, tab switching), Edit (clipboard and undo, so text fields keep working), Repository (Fetch, Pull, Push, Refresh, Read-only Mode, Reveal in Finder, Copy the Path), Branch (New Branch, Check Out / Merge into the Current Branch / Rebase onto as lists of the local branches, Stash), View (Search, Go to HEAD, density, Activity Log, Full Screen), Window. Entries that make no sense are greyed: no repository, read-only mode or a write running (reading actions stay available), a remote operation running, a detached HEAD for merge and rebase. Every action calls the function the toolbar or the context menu calls. Elsewhere than macOS the window is unchanged.

**Merges.** A merge commit is a small plain dot in the color of its branch: no initials, no ring, so that the people who did the work stand out.

## 20. Opening things outside the application

- **Pull requests.** *Open or create a pull request* (context menu of a branch, Branch menu for the current branch) reads the remotes (`list_remote_urls`), picks the first one on a known host (`origin` first; GitHub and Azure DevOps in https, ssh and `visualstudio.com` forms, `src/lib/remoteUrl.ts`), and opens the page that starts a pull request from the branch. With a token for the host it first looks for an **open pull request** of the branch through the API (`src-tauri/src/hosting.rs`: GitHub `pulls?head=`, Azure DevOps `pullrequests?searchCriteria.sourceRefName=`) and opens it instead, saying its number and title. A failed lookup (refused token, unknown repository, network) is reported and the creation page is opened anyway. Creating a pull request through the API is not done.
- **Tokens (decision of §12.1).** One optional token per host (`github.com`, `dev.azure.com`), kept in the **macOS keychain** (`keyring`, service `git-hydra`); the frontend can set, remove and ask whether one exists, never read it. Only the Rust side reads it, for the request to that host, over https. An encrypted file of the application was considered and set aside: its key would have to be somewhere on the disk too, so it protects less than the keychain.
- **Only `https` addresses and known applications.** `open_external` refuses anything but a plain `https://` address; the editor (`zed`, `vscode`, `cursor`) and the terminal (`terminal`, `iterm`, `ghostty`) are ids mapped to application names on the Rust side, and the path of a file must be inside the repository and exist.
- **Editor and terminal.** Chosen in *Settings, Integrations* (Zed and Terminal by default, `git-hydra:editor`, `git-hydra:terminal`); used by the Repository menu and by a right click on a file (commit and working tree panels).

## 21. Themes: code colors, fonts, files, System pair

- **Code colors in diffs** follow the theme: six tokens (keywords, strings, numbers, functions, types, comments) per variant, set as `--syntax-*` on the page. A theme without them (older, or imported) gets colors drawn from its lane colors by hue (`deriveSyntax`), comments use the muted text color. The editor shows them with a preview and warns when a color has a contrast under 3 with the background.
- **Fonts and size** are optional in a theme: interface font, code font, base size (12 to 16 to 20 px; everything is sized from it). Font lists accept names only (letters, digits, spaces, commas, quotes), so an imported theme cannot inject CSS. The density of the graph stays a setting of its own.
- **Import from a file** next to the paste of JSON (files up to 256 KB, same validation).
- **A theme for each look of the system.** In *System* mode two selectors pick the theme used when the system is light and when it is dark (`git-hydra:theme-id-light` / `-dark`; empty means the chosen theme). In *Light* and *Dark* the chosen theme is used as before. A deleted theme leaves the pair.
