import { computed, ref, shallowReactive, shallowRef } from "vue";
import {
  type ActivityEntry,
  type Area,
  type CommitDetail,
  type FileChange,
  type FileDiff,
  type GitErrorKind,
  type GraphRow,
  MAX_SEARCH_RESULTS,
  type RefInfo,
  type RefKind,
  type Snapshot,
  type StatusEntry,
  type WorkingStatus,
  toGitError,
} from "@/lib/api";
import { refsOfBranch } from "@/lib/branchRefs";
import { type GraphSource, type RepoBackend } from "@/lib/backend";
import type { Density } from "@/lib/metro/metrics";
import { createReadOnlyStore, type ReadOnlyStore } from "@/lib/readOnly";
import { createRecentStore, type RecentRepo, type RecentStore } from "@/lib/recents";

/** Result of the commit search of a tab. */
export interface SearchState {
  query: string;
  /** Matching rows, newest first. */
  rows: number[];
  /** Number of matching commits (can exceed `rows.length`). */
  total: number;
  /** Index in `rows` of the current match, -1 when there is none. */
  current: number;
  loading: boolean;
  error: string;
  /** Incremented for each search so late answers can be ignored. */
  request: number;
}

const emptySearch = (request: number): SearchState => ({
  query: "",
  rows: [],
  total: 0,
  current: -1,
  loading: false,
  error: "",
  request,
});

/** What the commit box holds. */
export interface CommitDraft {
  summary: string;
  body: string;
  amend: boolean;
  /** The message that was in the box before "amend" replaced it, to put back when unchecked. */
  beforeAmend: { summary: string; body: string } | null;
}

/** The last write that failed, for the panel that triggered it. */
export interface WriteError {
  kind: GitErrorKind;
  message: string;
}

/** A short message shown to the user, optionally with an action (Undo...). */
export interface Notice {
  id: number;
  kind: "info" | "error";
  text: string;
  action?: { label: string; run: () => void | Promise<void> };
}

const emptyDraft = (): CommitDraft => ({ summary: "", body: "", amend: false, beforeAmend: null });

/** One open repository. Only top-level properties are reactive (the graph data is large). */
export interface RepoTab {
  id: string;
  name: string;
  path: string;
  headBranch: string | null;
  /** Every ref of the repository, for the sidebar. */
  refs: RefInfo[];
  /** Full names of the refs hidden from the graph. */
  hidden: ReadonlySet<string>;
  source: GraphSource | null;
  loading: boolean;
  error: string;
  selected: number | null;
  selectedCommit: GraphRow | null;
  /** Vertical scroll of the graph, restored when coming back to the tab. */
  scrollTop: number;
  /** Incremented for each graph request so late answers can be ignored. */
  request: number;
  /** Message and files of the selected commit. */
  detail: CommitDetail | null;
  detailLoading: boolean;
  detailError: string;
  detailRequest: number;
  /** File whose diff is shown in place of the graph. */
  openFile: FileChange | null;
  /** Lane drawn in full while the others fade; `null` for no highlight. */
  highlightLane: number | null;
  search: SearchState;
  /** The commit at the top of the view and how far it is scrolled, to keep the place on refresh. */
  topAnchor: { id: string; offset: number } | null;
  /** Where to put the view when the graph is replaced by a refresh; `null` goes back to the top. */
  anchor: { row: number; offset: number } | null;
  refreshing: boolean;
  /** Another change arrived while refreshing: refresh again once done. */
  refreshQueued: boolean;
  /** Changes in the index and the working tree (`null` until read, or without a working tree). */
  status: WorkingStatus | null;
  statusRefreshing: boolean;
  statusQueued: boolean;
  /** The working tree row is selected: the panel shows the changes and the commit box. */
  wip: boolean;
  draft: CommitDraft;
  /** The changed file whose diff is shown in place of the graph. */
  workDiff: { entry: StatusEntry; area: Area } | null;
  /** Incremented when the status changes while a working diff is open, to reload it. */
  workDiffVersion: number;
  readOnly: boolean;
  /** A write is running: buttons that write are disabled. */
  busy: boolean;
  writeError: WriteError | null;
  /** A commit needs `user.name` / `user.email`: the identity dialog is shown. */
  identityRequired: boolean;
  snapshots: Snapshot[];
  activity: ActivityEntry[];
}

export type DiffMode = "unified" | "split";

const message = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

/** What to tell the user when a folder cannot be opened. */
export function describeOpenError(path: string, error: unknown): string {
  const text = message(error);
  return text.includes("cannot open repository")
    ? `${path} is not a git repository, or it cannot be read.`
    : text;
}

export function createWorkspace(
  backend: RepoBackend,
  recents: RecentStore,
  readOnlyStore: ReadOnlyStore = createReadOnlyStore(null),
) {
  const tabs = shallowRef<RepoTab[]>([]);
  const activeId = ref<string | null>(null);
  const density = ref<Density>("compact");
  const diffMode = ref<DiffMode>("unified");
  const recentRepos = ref<RecentRepo[]>(recents.list());
  /** Last error of an open attempt that did not produce a tab. */
  const openError = ref("");
  const opening = ref(false);

  const active = computed(() => tabs.value.find((tab) => tab.id === activeId.value) ?? null);

  async function loadGraph(tab: RepoTab): Promise<void> {
    const request = ++tab.request;
    tab.loading = true;
    tab.error = "";
    try {
      const source = await backend.graph(tab.id, [...tab.hidden]);
      if (request !== tab.request) return;
      tab.source = source;
      clearSelection(tab);
      tab.highlightLane = null;
      tab.anchor = null;
      tab.scrollTop = 0;
      // Rows changed: look for the same words in the new graph.
      if (tab.search.query.trim()) void search(tab, tab.search.query);
    } catch (error) {
      if (request !== tab.request) return;
      tab.error = message(error);
    } finally {
      if (request === tab.request) tab.loading = false;
    }
  }

  /**
   * Brings a tab up to date after the repository changed behind our back (commit, fetch,
   * checkout...), keeping the place in the history: the commit at the top of the view stays
   * there (unless the view was at the very top, which then shows the newest commits), and the
   * selected commit, its details, the open diff and the highlighted branch follow it.
   * Changes arriving meanwhile are grouped into one more refresh.
   */
  async function refreshGraph(tab: RepoTab): Promise<void> {
    if (tab.refreshing) {
      tab.refreshQueued = true;
      return;
    }
    tab.refreshing = true;
    try {
      do {
        tab.refreshQueued = false;
        await refreshOnce(tab);
      } while (tab.refreshQueued && tabs.value.includes(tab));
    } finally {
      tab.refreshing = false;
    }
  }

  async function refreshOnce(tab: RepoTab): Promise<void> {
    const request = ++tab.request;
    const selectedId = tab.selectedCommit?.id ?? null;
    const top = tab.topAnchor;
    const atTop = tab.scrollTop <= 1;
    const highlighted =
      tab.highlightLane === null ? undefined : tab.source?.meta.lanes[tab.highlightLane];
    tab.loading = true;
    try {
      const [source, refs] = await Promise.all([
        backend.graph(tab.id, [...tab.hidden]),
        backend.refs(tab.id),
      ]);
      if (request !== tab.request) return;

      const wanted = [selectedId, top?.id].filter((id): id is string => !!id);
      const found = wanted.length > 0 ? await backend.locate(tab.id, wanted) : [];
      if (request !== tab.request) return;
      const rowOf = (id: string | null | undefined): number | null => {
        const at = id ? wanted.indexOf(id) : -1;
        return at === -1 ? null : (found[at] ?? null);
      };
      const selectedRow = rowOf(selectedId);
      const topRow = rowOf(top?.id);

      // The selected row now has a new number: read it again for its current refs.
      let fresh: GraphRow | null = null;
      if (selectedRow !== null) {
        fresh = (await source.fetchPage(selectedRow, 1)).rows[0] ?? null;
        if (request !== tab.request) return;
      }

      tab.refs = refs;
      tab.source = source;
      if (atTop) tab.anchor = { row: 0, offset: 0 };
      else if (topRow !== null && top) tab.anchor = { row: topRow, offset: top.offset };
      else tab.anchor = { row: selectedRow ?? 0, offset: 0 };

      if (selectedRow !== null && fresh?.id === selectedId) {
        tab.selected = selectedRow;
        tab.selectedCommit = fresh;
      } else if (tab.selected !== null) {
        clearSelection(tab); // the commit is gone from the graph (or was never known)
      }

      // Lane numbers changed: find the highlighted branch again by name.
      tab.highlightLane =
        highlighted === undefined
          ? null
          : (source.meta.lanes.find(
              (l) => l.name === highlighted.name && l.kind === highlighted.kind,
            )?.id ?? null);
      tab.error = "";
      if (tab.search.query.trim()) void search(tab, tab.search.query);
      void refreshStatus(tab); // the branch or HEAD may have moved
    } catch (error) {
      if (request === tab.request) tab.error = message(error);
    } finally {
      if (request === tab.request) tab.loading = false;
    }
  }

  /** Refreshes tabs when their repository changes on disk. Returns a function to stop. */
  async function watchRepos(): Promise<() => void> {
    const tabOf = (repoId: string) => tabs.value.find((candidate) => candidate.id === repoId);
    const stops = await Promise.all([
      backend.onRepoChanged((repoId) => {
        const tab = tabOf(repoId);
        if (tab) void refreshGraph(tab);
      }),
      backend.onWorktreeChanged((repoId) => {
        const tab = tabOf(repoId);
        if (tab) void refreshStatus(tab, true);
      }),
      backend.onActivityChanged((repoId) => {
        const tab = tabOf(repoId);
        if (tab && activityOpen.value && tab.id === activeId.value) void loadActivity(tab);
      }),
    ]);
    return () => stops.forEach((stop) => stop());
  }

  // ---------- working tree: status, staging, commits ----------

  const notices = ref<Notice[]>([]);
  let noticeCount = 0;
  const activityOpen = ref(false);

  function dismissNotice(id: number): void {
    notices.value = notices.value.filter((notice) => notice.id !== id);
  }

  function notify(kind: Notice["kind"], text: string, action?: Notice["action"]): number {
    const id = ++noticeCount;
    notices.value = [...notices.value, { id, kind, text, action }];
    setTimeout(() => dismissNotice(id), kind === "error" ? 12000 : 9000);
    return id;
  }

  /**
   * Stores a new status. An open working diff is reloaded (`reloadDiff`: a file may have
   * changed without its status changing) or closed when its file left that list.
   */
  function applyStatus(tab: RepoTab, status: WorkingStatus, reloadDiff = false): void {
    const changed = JSON.stringify(tab.status) !== JSON.stringify(status);
    if (changed) tab.status = status;
    const open = tab.workDiff;
    if (!open) return;
    const list = open.area === "staged" ? status.staged : status.unstaged;
    if (!list.some((entry) => entry.path === open.entry.path)) tab.workDiff = null;
    else if (changed || reloadDiff) tab.workDiffVersion++;
  }

  /** Reads the status again; changes arriving meanwhile are grouped into one more read. */
  async function refreshStatus(tab: RepoTab, reloadDiff = false): Promise<void> {
    if (tab.statusRefreshing) {
      tab.statusQueued = true;
      return;
    }
    tab.statusRefreshing = true;
    try {
      do {
        tab.statusQueued = false;
        try {
          applyStatus(tab, await backend.status(tab.id), reloadDiff);
        } catch {
          // A repository without a working tree, or a transient failure: keep what we have.
        }
      } while (tab.statusQueued && tabs.value.includes(tab));
    } finally {
      tab.statusRefreshing = false;
    }
  }

  /** Selects the working tree row: the panel shows the changes and the commit box. */
  function selectWip(tab: RepoTab): void {
    clearSelection(tab);
    tab.wip = true;
    tab.workDiff = null;
    void refreshStatus(tab);
  }

  const openWorkDiff = (tab: RepoTab, entry: StatusEntry, area: Area): void => {
    tab.workDiff = { entry, area };
    tab.openFile = null;
  };

  const closeWorkDiff = (tab: RepoTab): void => {
    tab.workDiff = null;
  };

  /** Diff of the open working tree file. */
  function workingDiff(tab: RepoTab, force = false): Promise<FileDiff> {
    if (!tab.workDiff) return Promise.reject(new Error("no file selected"));
    return backend.workingDiff(tab.id, tab.workDiff.entry, tab.workDiff.area, force);
  }

  /** Kinds of failure the panel that triggered the write shows itself, without a notice. */
  const INLINE_ERRORS: GitErrorKind[] = [
    "hookFailed",
    "signingFailed",
    "nothingToCommit",
    "needsIdentity",
  ];

  function handleWriteError(tab: RepoTab, error: unknown): void {
    const failure = toGitError(error);
    tab.writeError = { kind: failure.kind, message: failure.message };
    if (failure.kind === "needsIdentity") tab.identityRequired = true;
    if (failure.kind === "staleHunk") void refreshStatus(tab, true);
    if (failure.kind === "readOnly") tab.readOnly = true;
    if (!INLINE_ERRORS.includes(failure.kind)) notify("error", failure.message);
  }

  /** Runs a write: refused at once in read-only mode, with failures reported. `null` on failure. */
  async function write<T>(tab: RepoTab, action: () => Promise<T>): Promise<T | null> {
    if (tab.readOnly) {
      handleWriteError(tab, { kind: "readOnly", message: "This repository is in read-only mode." });
      return null;
    }
    tab.busy = true;
    tab.writeError = null;
    try {
      return await action();
    } catch (error) {
      handleWriteError(tab, error);
      return null;
    } finally {
      tab.busy = false;
    }
  }

  async function writeStatus(tab: RepoTab, action: () => Promise<WorkingStatus>): Promise<void> {
    const status = await write(tab, action);
    if (status) applyStatus(tab, status);
  }

  const stage = (tab: RepoTab, paths: string[]) =>
    writeStatus(tab, () => backend.stage(tab.id, paths));
  const unstage = (tab: RepoTab, paths: string[]) =>
    writeStatus(tab, () => backend.unstage(tab.id, paths));
  const stageHunk = (tab: RepoTab, path: string, hunkId: string) =>
    writeStatus(tab, () => backend.stageHunk(tab.id, path, hunkId));
  const unstageHunk = (tab: RepoTab, path: string, hunkId: string) =>
    writeStatus(tab, () => backend.unstageHunk(tab.id, path, hunkId));

  /** Throws away the unstaged changes of files; Undo (in the notice) brings them back. */
  async function discard(tab: RepoTab, paths: string[]): Promise<void> {
    const result = await write(tab, () => backend.discard(tab.id, paths));
    if (!result) return;
    applyStatus(tab, result.status);
    const snapshot = result.snapshot;
    if (!snapshot) return;
    const count = snapshot.paths.length;
    notify("info", `Discarded ${count} file${count === 1 ? "" : "s"}.`, {
      label: "Undo",
      run: () => restoreSnapshot(tab, snapshot.reference),
    });
    void loadSnapshots(tab);
  }

  async function restoreSnapshot(tab: RepoTab, reference: string): Promise<void> {
    const result = await write(tab, () => backend.restoreSnapshot(tab.id, reference));
    if (!result) return;
    applyStatus(tab, result.status, true);
    notify("info", "Changes restored.");
    void loadSnapshots(tab);
  }

  async function loadSnapshots(tab: RepoTab): Promise<void> {
    try {
      tab.snapshots = await backend.snapshots(tab.id);
    } catch {
      tab.snapshots = [];
    }
  }

  /** Changes the summary or the description in the commit box. */
  const updateDraft = (
    tab: RepoTab,
    patch: Partial<Pick<CommitDraft, "summary" | "body">>,
  ): void => {
    tab.draft = { ...tab.draft, ...patch };
  };

  /** Commits the staged changes (or amends the last commit) with the message in the draft. */
  async function commit(tab: RepoTab): Promise<void> {
    const { summary, body, amend } = tab.draft;
    const result = await write(tab, () => backend.commit(tab.id, summary, body, amend));
    if (!result) return;
    applyStatus(tab, result.status);
    tab.draft = emptyDraft();
    const short = result.id.slice(0, 7);
    notify("info", `${amend ? "Amended" : "Committed"} ${short}.`, {
      label: "Undo",
      run: () => undoLastCommit(tab, result.before, result.id),
    });
  }

  async function undoLastCommit(tab: RepoTab, before: string | null, after: string): Promise<void> {
    await writeStatus(tab, () => backend.undoCommit(tab.id, before, after));
    if (!tab.writeError) notify("info", "Commit undone: its changes are staged again.");
  }

  /** Turns "amend" on (filling the box with the last message) or off (putting back the draft). */
  async function setAmend(tab: RepoTab, amend: boolean): Promise<void> {
    if (amend === tab.draft.amend) return;
    if (!amend) {
      const saved = tab.draft.beforeAmend ?? { summary: "", body: "" };
      tab.draft = { ...saved, amend: false, beforeAmend: null };
      return;
    }
    const head = tab.status?.head;
    const beforeAmend = { summary: tab.draft.summary, body: tab.draft.body };
    let { summary, body } = beforeAmend;
    if (head) {
      try {
        const message = (await backend.commitDetail(tab.id, head)).message;
        const newline = message.indexOf("\n");
        summary = newline === -1 ? message : message.slice(0, newline);
        body = newline === -1 ? "" : message.slice(newline + 1).trim();
      } catch (error) {
        handleWriteError(tab, error);
        return;
      }
    }
    tab.draft = { summary, body, amend: true, beforeAmend };
  }

  /** Saves the identity asked for by a commit, then tries the commit again. */
  async function submitIdentity(
    tab: RepoTab,
    name: string,
    email: string,
    global: boolean,
  ): Promise<void> {
    const identity = await write(tab, () => backend.setIdentity(tab.id, name, email, global));
    if (!identity) return;
    tab.identityRequired = false;
    if (tab.draft.summary.trim()) await commit(tab);
  }

  const cancelIdentity = (tab: RepoTab): void => {
    tab.identityRequired = false;
    tab.writeError = null;
  };

  async function setReadOnly(tab: RepoTab, readOnly: boolean): Promise<void> {
    try {
      await backend.setReadOnly(tab.id, readOnly);
      tab.readOnly = readOnly;
      readOnlyStore.set(tab.id, readOnly);
    } catch (error) {
      notify("error", toGitError(error).message);
    }
  }

  async function loadActivity(tab: RepoTab): Promise<void> {
    try {
      tab.activity = await backend.activity(tab.id);
    } catch {
      tab.activity = [];
    }
  }

  async function clearActivity(tab: RepoTab): Promise<void> {
    await backend.clearActivity(tab.id).catch(() => {});
    tab.activity = [];
  }

  async function openPath(path: string): Promise<void> {
    const trimmed = path.trim();
    if (!trimmed) return;
    opening.value = true;
    openError.value = "";
    try {
      const { info, refs } = await backend.open(trimmed);
      const existing = tabs.value.find((tab) => tab.id === info.id);
      if (existing) {
        activeId.value = existing.id;
        return;
      }
      const tab = shallowReactive<RepoTab>({
        id: info.id,
        name: info.name,
        path: info.path,
        headBranch: info.headBranch,
        refs,
        hidden: new Set(),
        source: null,
        loading: false,
        error: "",
        selected: null,
        selectedCommit: null,
        scrollTop: 0,
        request: 0,
        detail: null,
        detailLoading: false,
        detailError: "",
        detailRequest: 0,
        openFile: null,
        highlightLane: null,
        search: emptySearch(0),
        topAnchor: null,
        anchor: null,
        refreshing: false,
        refreshQueued: false,
        status: null,
        statusRefreshing: false,
        statusQueued: false,
        wip: false,
        draft: emptyDraft(),
        workDiff: null,
        workDiffVersion: 0,
        readOnly: readOnlyStore.has(info.id),
        busy: false,
        writeError: null,
        identityRequired: false,
        snapshots: [],
        activity: [],
      });
      tabs.value = [...tabs.value, tab];
      activeId.value = tab.id;
      recentRepos.value = recents.add({ path: info.path, name: info.name, openedAt: Date.now() });
      if (tab.readOnly) void backend.setReadOnly(tab.id, true).catch(() => {});
      void refreshStatus(tab);
      await loadGraph(tab);
    } catch (error) {
      openError.value = describeOpenError(trimmed, error);
    } finally {
      opening.value = false;
    }
  }

  async function pickAndOpen(): Promise<void> {
    try {
      const path = await backend.pickFolder();
      if (path) await openPath(path);
    } catch (error) {
      openError.value = message(error);
    }
  }

  async function closeTab(id: string): Promise<void> {
    const index = tabs.value.findIndex((tab) => tab.id === id);
    if (index === -1) return;
    const remaining = tabs.value.filter((tab) => tab.id !== id);
    tabs.value = remaining;
    if (activeId.value === id) {
      activeId.value = (remaining[index] ?? remaining[index - 1] ?? null)?.id ?? null;
    }
    try {
      await backend.close(id);
    } catch {
      // The tab is gone either way.
    }
  }

  function activate(id: string): void {
    if (tabs.value.some((tab) => tab.id === id)) activeId.value = id;
  }

  async function setHidden(tab: RepoTab, fullNames: string[], hide: boolean): Promise<void> {
    const next = new Set(tab.hidden);
    for (const name of fullNames) {
      if (hide) next.add(name);
      else next.delete(name);
    }
    tab.hidden = next;
    await loadGraph(tab);
  }

  /** Shows every hidden ref again. */
  const showAllRefs = (tab: RepoTab): Promise<void> => setHidden(tab, [...tab.hidden], false);

  const toggleRef = (tab: RepoTab, fullName: string) =>
    setHidden(tab, [fullName], !tab.hidden.has(fullName));

  /** Hides a whole kind of refs, or shows it again when everything of that kind is hidden. */
  function toggleKind(tab: RepoTab, kind: RefKind): Promise<void> {
    const names = tab.refs.filter((ref) => ref.kind === kind).map((ref) => ref.fullName);
    const allHidden = names.every((name) => tab.hidden.has(name));
    return setHidden(tab, names, !allHidden);
  }

  function clearSelection(tab: RepoTab): void {
    tab.selected = null;
    tab.selectedCommit = null;
    tab.detail = null;
    tab.detailLoading = false;
    tab.detailError = "";
    tab.detailRequest++;
    tab.openFile = null;
  }

  async function loadDetail(tab: RepoTab, commitId: string): Promise<void> {
    const request = ++tab.detailRequest;
    tab.detail = null;
    tab.detailLoading = true;
    tab.detailError = "";
    try {
      const detail = await backend.commitDetail(tab.id, commitId);
      if (request === tab.detailRequest) tab.detail = detail;
    } catch (error) {
      if (request === tab.detailRequest) tab.detailError = message(error);
    } finally {
      if (request === tab.detailRequest) tab.detailLoading = false;
    }
  }

  /**
   * Selects a row. The commit may arrive in a second call once its row is loaded; the details
   * (message, files) are then fetched. Selecting another commit closes the open diff.
   */
  function select(tab: RepoTab, row: number | null, commit: GraphRow | null = null): void {
    if (row === null) return clearSelection(tab);
    tab.wip = false;
    tab.workDiff = null;
    if (row !== tab.selected) {
      clearSelection(tab);
      tab.selected = row;
    }
    if (commit && tab.selectedCommit?.id !== commit.id) {
      tab.selectedCommit = commit;
      void loadDetail(tab, commit.id);
    }
  }

  /** Searches the commits of the tab's graph; results are kept in `tab.search`. */
  async function search(tab: RepoTab, query: string): Promise<void> {
    const request = tab.search.request + 1;
    if (!query.trim()) {
      tab.search = emptySearch(request);
      return;
    }
    tab.search = { ...tab.search, query, loading: true, error: "", request };
    try {
      const result = await backend.search(tab.id, query, MAX_SEARCH_RESULTS);
      if (request !== tab.search.request) return;
      tab.search = {
        query,
        rows: result.rows,
        total: result.total,
        current: result.rows.length > 0 ? 0 : -1,
        loading: false,
        error: "",
        request,
      };
    } catch (error) {
      if (request !== tab.search.request) return;
      tab.search = { ...emptySearch(request), query, error: message(error) };
    }
  }

  /** Moves to the next (or previous) match; returns its row, `null` when there is none. */
  function stepMatch(tab: RepoTab, delta: 1 | -1): number | null {
    const { rows, current } = tab.search;
    if (rows.length === 0) return null;
    const next = current < 0 ? 0 : (current + delta + rows.length) % rows.length;
    tab.search = { ...tab.search, current: next };
    return rows[next];
  }

  const clearSearch = (tab: RepoTab): void => {
    tab.search = emptySearch(tab.search.request + 1);
  };

  /** Highlights a lane, or removes the highlight if it is already the highlighted one. */
  const toggleHighlight = (tab: RepoTab, lane: number): void => {
    tab.highlightLane = tab.highlightLane === lane ? null : lane;
  };

  const clearHighlight = (tab: RepoTab): void => {
    tab.highlightLane = null;
  };

  /** Hides the branch of a lane: its local branch and the remote-tracking ones. */
  function hideBranch(tab: RepoTab, laneName: string): Promise<void> {
    const names = refsOfBranch(tab.refs, laneName).map((ref) => ref.fullName);
    return names.length > 0 ? setHidden(tab, names, true) : Promise.resolve();
  }

  const openFile = (tab: RepoTab, file: FileChange | null): void => {
    tab.openFile = file;
  };

  /** Diff of the open file of the selected commit. */
  function fileDiff(tab: RepoTab, force = false): Promise<FileDiff> {
    if (!tab.selectedCommit || !tab.openFile) return Promise.reject(new Error("no file selected"));
    return backend.fileDiff(tab.id, tab.selectedCommit.id, tab.openFile, force);
  }

  function forgetRecent(path: string): void {
    recentRepos.value = recents.remove(path);
  }

  return {
    notices,
    activityOpen,
    notify,
    dismissNotice,
    refreshStatus,
    selectWip,
    openWorkDiff,
    closeWorkDiff,
    workingDiff,
    stage,
    unstage,
    stageHunk,
    unstageHunk,
    discard,
    restoreSnapshot,
    loadSnapshots,
    commit,
    updateDraft,
    undoLastCommit,
    setAmend,
    submitIdentity,
    cancelIdentity,
    setReadOnly,
    loadActivity,
    clearActivity,
    tabs,
    activeId,
    active,
    density,
    diffMode,
    recentRepos,
    openError,
    opening,
    openPath,
    refreshGraph,
    retry: loadGraph,
    showAllRefs,
    watchRepos,
    pickAndOpen,
    closeTab,
    activate,
    toggleRef,
    toggleKind,
    select,
    openFile,
    fileDiff,
    search,
    stepMatch,
    clearSearch,
    toggleHighlight,
    clearHighlight,
    hideBranch,
    forgetRecent,
  };
}

export type Workspace = ReturnType<typeof createWorkspace>;

let shared: Workspace | undefined;

/** The app-wide workspace; the backend is supplied once at startup. */
export function useWorkspace(backend?: RepoBackend): Workspace {
  if (!shared) {
    if (!backend) throw new Error("the workspace must be created with a backend first");
    let storage: Storage | null;
    try {
      storage = window.localStorage;
    } catch {
      storage = null;
    }
    shared = createWorkspace(backend, createRecentStore(storage), createReadOnlyStore(storage));
  }
  return shared;
}
