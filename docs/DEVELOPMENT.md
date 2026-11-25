# Developing Gitsune

Tauri 2 (Rust) + Vue 3 + TypeScript + Tailwind + shadcn-vue. All design decisions, measurements and the milestone plan are in [SPEC.md](../SPEC.md).

## Requirements

- [Bun](https://bun.sh), a Rust toolchain (edition 2024; the CI uses Rust 1.95.0, see `.github/workflows/ci.yml`) and, on macOS, the Xcode command line tools. Linux needs the WebKitGTK and GTK development packages, see the CI file for the list.
- `git` on the path: the application runs it for every change, and the tests build real repositories with it.

## Everyday commands

```bash
bun install
bun run tauri dev        # the app, with hot reload
bun run dev              # the UI alone in a browser, on recorded demo data (no Tauri needed)
bun run check:all        # typecheck, lint, format, front tests, rustfmt, clippy, Rust tests
bun run package          # release build: .app and .dmg under src-tauri/target/release/bundle
```

In the plain browser mode the app shows a small invented repository. In the browser console, `__simulateRepoChange()` adds a commit on top, like the file watcher does for a real repository.

The release build is not signed or notarized: it runs on the machine that built it; to share it, sign it first. A tag `vX.Y.Z` (or a manual run) makes the CI package for macOS, Linux and Windows; Linux and Windows are experimental, see section 22 of the SPEC.

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

| Path                              | What                                                                                         |
| --------------------------------- | -------------------------------------------------------------------------------------------- |
| `src-tauri/src/layout`            | Lane layout engine: topological order, lanes, connectors, names, colors                      |
| `src-tauri/src/git`               | Git access: reads with `gix`, writes through the `git` binary; one module per kind of action |
| `src-tauri/src/commands.rs`       | Tauri commands and the registry of open repositories                                         |
| `src-tauri/src/hosting*.rs`       | Pull requests on GitHub, GitLab and Azure DevOps, with the sign-in of `gh`, `glab` and `az`  |
| `src-tauri/src/platform.rs`       | What differs between macOS, Linux and Windows                                                |
| `src/lib/metro`                   | Canvas renderer: geometry, drawing, palette, row cache                                       |
| `src/composables/useWorkspace.ts` | Application state: tabs, selection, search, refresh, every action on a repository            |
| `src/components`                  | Vue components (graph, sidebar, commit panel, diff view, dialogs...)                         |
| `src/dev`                         | Demo data and stand-in backend for browser development (not in production builds)            |
| `scripts`                         | Synthetic repository and icon generators                                                     |
