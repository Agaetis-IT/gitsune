import { computed, ref, shallowReactive, shallowRef } from "vue";
import {
  type ActivityEntry,
  type Area,
  type CheckoutTarget,
  type CommitDetail,
  type DeletedBranch,
  type DeletedTag,
  type DirtyStrategy,
  type FileChange,
  type FileDiff,
  type GitErrorKind,
  type GraphRow,
  MAX_SEARCH_RESULTS,
  type PullStrategy,
  type PushOptions,
  type RefInfo,
  type RemoteResult,
  type RefKind,
  type ResetMode,
  type StashEntry,
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
  /** Files the error is about (those a checkout would overwrite). */
  files: string[];
}

/** A commit, as the target of a new branch or tag. */
export interface CommitTarget {
  id: string;
  /** What to call it to the user (a short id, or a branch name). */
  label: string;
}

/** The question the branch and tag dialogs ask. */
export type BranchDialog =
  | { type: "createBranch"; start: CommitTarget | null }
  | { type: "renameBranch"; name: string }
  /** `unmerged` holds git's warning once it refused to delete a branch with unmerged commits. */
  | { type: "deleteBranch"; name: string; unmerged: string | null }
  | { type: "createTag"; target: CommitTarget | null }
  | { type: "deleteTag"; name: string }
  | { type: "upstream"; branch: string }
  /** A checkout would overwrite local changes: stash them, discard them, or give up. */
  | { type: "dirtyCheckout"; target: CheckoutTarget; files: string[]; message: string }
  /** Where to push; `reason` is why this question is asked (no upstream yet, push refused). */
  | {
      type: "push";
      remotes: string[];
      remote: string;
      branch: string;
      setUpstream: boolean;
      reason: string | null;
      /** The remote has commits we do not: pulling first, or forcing with a lease, are options. */
      canForce: boolean;
    }
  /** The branches diverged and git was not told how to reconcile them. */
  | { type: "pullStrategy"; message: string }
  | { type: "deleteRemoteBranch"; remote: string; branch: string }
  /** Put the changes of the working tree aside. */
  | { type: "stash" }
  | { type: "dropStash"; stash: StashEntry }
  /** Move the current branch to a commit: soft, mixed or hard. */
  | { type: "reset"; target: CommitTarget };

/** A fetch, pull or push that is running. */
export interface RemoteOp {
  id: string;
  kind: "fetch" | "pull" | "push" | "deleteRemote";
  /** What is happening, for the progress panel ("Pushing main…"). */
  label: string;
  /** The last line git printed. */
  line: string;
  /** 0 to 100 when git reports a percentage, otherwise `null`. */
  percent: number | null;
  /** A question git or ssh is waiting for an answer to. */
  prompt: { promptId: string; text: string } | null;
  cancelling: boolean;
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
  /** The branch or tag dialog being shown, if any. */
  dialog: BranchDialog | null;
  /** The fetch, pull or push in progress, if any. */
  remote: RemoteOp | null;
  /** The stash list, most recent first. */
  stashes: StashEntry[];
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
    void loadStashes(tab);
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
      backend.onRemoteProgress((event) => {
        const tab = tabOf(event.repoId);
        if (tab?.remote?.id !== event.opId) return;
        const percent = /(\d{1,3})%/.exec(event.line);
        tab.remote = {
          ...tab.remote,
          line: event.line,
          percent: percent ? Math.min(100, Number(percent[1])) : tab.remote.percent,
        };
      }),
      backend.onAskpass((event) => {
        const tab = tabOf(event.repoId);
        if (tab?.remote?.id !== event.opId) return;
        tab.remote = { ...tab.remote, prompt: { promptId: event.promptId, text: event.prompt } };
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
    "invalidName",
    "alreadyExists",
    "wouldOverwriteChanges",
    "notFullyMerged",
    // The remote flows explain these themselves, with a dialog or a notice of their own.
    "noUpstream",
    "divergedBranches",
    "pushRejected",
    "conflict",
  ];

  function handleWriteError(tab: RepoTab, error: unknown): void {
    const failure = toGitError(error);
    tab.writeError = { kind: failure.kind, message: failure.message, files: failure.files };
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

  // ---------- branches, checkout and tags ----------

  const openDialog = (tab: RepoTab, dialog: BranchDialog): void => {
    tab.writeError = null;
    tab.dialog = dialog;
  };

  const closeDialog = (tab: RepoTab): void => {
    tab.dialog = null;
    tab.writeError = null;
  };

  const askCreateBranch = (tab: RepoTab, start: CommitTarget | null) =>
    openDialog(tab, { type: "createBranch", start });
  const askRenameBranch = (tab: RepoTab, name: string) =>
    openDialog(tab, { type: "renameBranch", name });
  const askDeleteBranch = (tab: RepoTab, name: string) =>
    openDialog(tab, { type: "deleteBranch", name, unmerged: null });
  const askCreateTag = (tab: RepoTab, target: CommitTarget | null) =>
    openDialog(tab, { type: "createTag", target });
  const askDeleteTag = (tab: RepoTab, name: string) => openDialog(tab, { type: "deleteTag", name });
  const askUpstream = (tab: RepoTab, branch: string) =>
    openDialog(tab, { type: "upstream", branch });

  const targetLabel = (target: CheckoutTarget): string =>
    target.kind === "commit" ? target.id.slice(0, 7) : target.name;

  /**
   * Switches branch (or detaches HEAD). When local changes are in the way a dialog offers to
   * stash or discard them; `strategy` is the answer to that dialog.
   */
  async function checkout(
    tab: RepoTab,
    target: CheckoutTarget,
    strategy: DirtyStrategy | null = null,
  ): Promise<void> {
    const result = await write(tab, () => backend.checkout(tab.id, target, strategy));
    if (!result) {
      const failure = tab.writeError;
      if (failure?.kind === "wouldOverwriteChanges") {
        tab.dialog = {
          type: "dirtyCheckout",
          target,
          files: failure.files,
          message: failure.message,
        };
      }
      return;
    }
    closeDialog(tab);
    applyStatus(tab, result.status, true);
    const label = targetLabel(target);
    const where = result.status.detached ? `Detached HEAD at ${label}` : `Switched to ${label}`;
    const snapshot = result.snapshot;
    if (result.stashConflict) {
      notify(
        "error",
        `${where}, but applying your stashed changes conflicted. They are still in the stash (git stash list).`,
      );
    } else if (snapshot) {
      notify("info", `${where}. Your changes were discarded.`, {
        label: "Undo",
        run: () => restoreSnapshot(tab, snapshot.reference),
      });
    } else if (result.stashed) {
      notify("info", `${where}. Your changes were stashed and applied again.`);
    } else {
      notify("info", `${where}.`);
    }
  }

  async function createBranch(
    tab: RepoTab,
    name: string,
    start: string | null,
    checkout: boolean,
  ): Promise<boolean> {
    const status = await write(tab, () => backend.createBranch(tab.id, name, start, checkout));
    if (!status) return false;
    closeDialog(tab);
    applyStatus(tab, status, true);
    notify("info", checkout ? `Created and switched to ${name}.` : `Created branch ${name}.`);
    return true;
  }

  async function renameBranch(tab: RepoTab, oldName: string, newName: string): Promise<boolean> {
    const status = await write(tab, () => backend.renameBranch(tab.id, oldName, newName));
    if (!status) return false;
    closeDialog(tab);
    applyStatus(tab, status);
    notify("info", `Renamed ${oldName} to ${newName}.`);
    return true;
  }

  /**
   * Deletes a branch. A branch with commits that are not merged anywhere else is first
   * refused (the dialog then shows git's warning); `force` is the answer to that warning.
   */
  async function deleteBranch(tab: RepoTab, name: string, force = false): Promise<void> {
    const deleted = await write(tab, () => backend.deleteBranch(tab.id, name, force));
    if (!deleted) {
      const failure = tab.writeError;
      if (failure?.kind === "notFullyMerged") {
        tab.dialog = { type: "deleteBranch", name, unmerged: failure.message };
      }
      return;
    }
    closeDialog(tab);
    applyStatus(tab, deleted.status);
    notify("info", `Deleted branch ${name}.`, {
      label: "Undo",
      run: () => restoreDeletedBranch(tab, deleted),
    });
  }

  async function restoreDeletedBranch(tab: RepoTab, deleted: DeletedBranch): Promise<void> {
    await writeStatus(tab, () =>
      backend.restoreBranch(tab.id, deleted.name, deleted.tip, deleted.upstream),
    );
    if (!tab.writeError) notify("info", `Branch ${deleted.name} is back.`);
  }

  async function setUpstream(
    tab: RepoTab,
    branch: string,
    upstream: string | null,
  ): Promise<boolean> {
    const status = await write(tab, () => backend.setUpstream(tab.id, branch, upstream));
    if (!status) return false;
    closeDialog(tab);
    applyStatus(tab, status);
    notify(
      "info",
      upstream ? `${branch} now tracks ${upstream}.` : `${branch} no longer tracks a branch.`,
    );
    return true;
  }

  async function createTag(
    tab: RepoTab,
    name: string,
    target: string | null,
    message: string | null,
  ): Promise<boolean> {
    const status = await write(tab, () => backend.createTag(tab.id, name, target, message));
    if (!status) return false;
    closeDialog(tab);
    applyStatus(tab, status);
    notify("info", `Created tag ${name}.`);
    return true;
  }

  async function deleteTag(tab: RepoTab, name: string): Promise<void> {
    const deleted = await write(tab, () => backend.deleteTag(tab.id, name));
    if (!deleted) return;
    closeDialog(tab);
    applyStatus(tab, deleted.status);
    notify("info", `Deleted tag ${name}.`, {
      label: "Undo",
      run: () => restoreDeletedTag(tab, deleted),
    });
  }

  async function restoreDeletedTag(tab: RepoTab, deleted: DeletedTag): Promise<void> {
    await writeStatus(tab, () => backend.restoreTag(tab.id, deleted.name, deleted.target));
    if (!tab.writeError) notify("info", `Tag ${deleted.name} is back.`);
  }

  // ---------- fetch, pull and push ----------

  let opCount = 0;

  /** The same git message, whatever the phrase, tells a push that needs a pull first. */
  const BEHIND = /non-fast-forward|fetch first|stale info|behind/i;

  /**
   * Runs a fetch, pull or push: one at a time per repository, with progress and a cancel
   * button in the panel. Pull and push are writes (refused in read-only mode); a fetch only
   * moves remote-tracking refs, so it is allowed. `null` on failure, the cause in `writeError`.
   */
  async function runRemote(
    tab: RepoTab,
    kind: RemoteOp["kind"],
    label: string,
    action: (opId: string) => Promise<RemoteResult>,
  ): Promise<RemoteResult | null> {
    if (tab.remote) return null;
    if (tab.readOnly && kind !== "fetch") {
      handleWriteError(tab, { kind: "readOnly", message: "This repository is in read-only mode." });
      return null;
    }
    const id = `op-${Date.now().toString(36)}-${++opCount}`;
    tab.remote = { id, kind, label, line: "", percent: null, prompt: null, cancelling: false };
    tab.writeError = null;
    if (kind !== "fetch") tab.busy = true;
    try {
      const result = await action(id);
      applyStatus(tab, result.status);
      return result;
    } catch (error) {
      const failure = toGitError(error);
      if (failure.kind === "cancelled") {
        tab.writeError = null;
        notify("info", `${label.replace(/…$/, "")} cancelled.`);
      } else {
        handleWriteError(tab, failure);
      }
      return null;
    } finally {
      tab.remote = null;
      if (kind !== "fetch") tab.busy = false;
    }
  }

  /** Errors the remote flows explain with a notice or a dialog of their own. */
  function remoteFailure(tab: RepoTab): WriteError | null {
    const failure = tab.writeError;
    if (failure?.kind === "wouldOverwriteChanges") notify("error", failure.message);
    return failure;
  }

  async function fetchRemote(tab: RepoTab, remote: string | null = null, prune = false) {
    const result = await runRemote(tab, "fetch", "Fetching…", (opId) =>
      backend.fetch(tab.id, opId, remote, prune),
    );
    if (result) {
      notify(
        "info",
        result.summary.trim() ? "Fetched the remotes." : "Nothing new on the remotes.",
      );
    }
  }

  async function pull(tab: RepoTab, strategy: PullStrategy | null = null): Promise<void> {
    const result = await runRemote(tab, "pull", "Pulling…", (opId) =>
      backend.pull(tab.id, opId, strategy),
    );
    if (result) {
      closeDialog(tab);
      notify(
        "info",
        /already up to date/i.test(result.summary)
          ? "Already up to date."
          : "Pulled the latest changes.",
      );
      return;
    }
    const failure = remoteFailure(tab);
    if (failure?.kind === "divergedBranches") {
      tab.dialog = { type: "pullStrategy", message: failure.message };
    } else if (failure?.kind === "noUpstream") {
      notify(
        "error",
        "This branch has no upstream to pull from. Push it first, or set its upstream.",
      );
    } else if (failure?.kind === "conflict") {
      notify(
        "error",
        "The merge stopped on conflicts. Resolve them in a terminal for now: conflict resolution comes in a later version.",
      );
    }
  }

  async function openPushDialog(
    tab: RepoTab,
    branch: string | null,
    reason: string | null,
    canForce: boolean,
    remote: string | null = null,
  ): Promise<void> {
    const remotes = await backend.remotes(tab.id).catch(() => [] as string[]);
    const name = branch ?? tab.headBranch ?? "";
    const own = tab.refs.find((ref) => ref.kind === "remote" && ref.name.endsWith(`/${name}`));
    const chosen =
      remote ??
      (own ? own.name.slice(0, own.name.length - name.length - 1) : null) ??
      remotes[0] ??
      "";
    tab.writeError = null;
    tab.dialog = {
      type: "push",
      remotes,
      remote: remotes.includes(chosen) ? chosen : (remotes[0] ?? ""),
      branch: name,
      setUpstream: reason !== null && !canForce,
      reason,
      canForce,
    };
  }

  /** Asks where to push the branch (the "Push…" menu entry). */
  const askPush = (tab: RepoTab, branch: string | null = null) =>
    openPushDialog(tab, branch, null, false);

  async function push(tab: RepoTab, options: Partial<PushOptions> = {}): Promise<void> {
    const full: PushOptions = {
      remote: options.remote ?? null,
      branch: options.branch ?? null,
      setUpstream: options.setUpstream ?? false,
      forceWithLease: options.forceWithLease ?? false,
    };
    const what = full.branch ?? tab.headBranch ?? "the branch";
    const result = await runRemote(tab, "push", `Pushing ${what}…`, (opId) =>
      backend.push(tab.id, opId, full),
    );
    if (result) {
      closeDialog(tab);
      notify(
        "info",
        /up-to-date/i.test(result.summary) ? "Everything is up to date." : `Pushed ${what}.`,
      );
      return;
    }
    const failure = remoteFailure(tab);
    if (failure?.kind === "noUpstream") {
      await openPushDialog(
        tab,
        full.branch,
        "This branch has no upstream yet: choose the remote to push it to.",
        false,
        full.remote,
      );
    } else if (failure?.kind === "pushRejected") {
      if (BEHIND.test(failure.message)) {
        await openPushDialog(
          tab,
          full.branch,
          "The remote has commits you do not have. Pull them first, or overwrite the remote branch only if nobody else pushed since your last fetch.",
          true,
          full.remote,
        );
      } else {
        notify("error", failure.message);
      }
    }
  }

  async function deleteRemoteBranch(tab: RepoTab, remote: string, branch: string): Promise<void> {
    const result = await runRemote(tab, "deleteRemote", `Deleting ${remote}/${branch}…`, (opId) =>
      backend.deleteRemoteBranch(tab.id, opId, remote, branch),
    );
    if (result) {
      closeDialog(tab);
      notify("info", `Deleted ${branch} on ${remote}.`);
    } else if (remoteFailure(tab)?.kind === "pushRejected") {
      notify("error", tab.writeError?.message ?? "The remote refused the deletion.");
    }
  }

  /** Splits `origin/feature/x` into its remote and its branch (remote names have no slash). */
  const askDeleteRemoteBranch = (tab: RepoTab, remote: string, branch: string) =>
    openDialog(tab, { type: "deleteRemoteBranch", remote, branch });

  async function cancelRemote(tab: RepoTab): Promise<void> {
    const op = tab.remote;
    if (!op || op.cancelling) return;
    tab.remote = { ...op, cancelling: true, prompt: null };
    await backend.cancelOperation(op.id).catch(() => {});
  }

  /** Answers the question git is waiting for; `null` refuses, which ends the operation. */
  async function answerPrompt(tab: RepoTab, answer: string | null): Promise<void> {
    const op = tab.remote;
    if (!op?.prompt) return;
    const { promptId } = op.prompt;
    tab.remote = { ...op, prompt: null };
    await backend.answerAskpass(op.id, promptId, answer).catch(() => {});
  }

  // ---------- stash, cherry-pick, revert and reset ----------

  async function loadStashes(tab: RepoTab): Promise<void> {
    try {
      tab.stashes = await backend.stashes(tab.id);
    } catch {
      tab.stashes = [];
    }
  }

  /** The failures the panels explain themselves are shown as a notice for these flows. */
  function reportInline(tab: RepoTab): void {
    const failure = tab.writeError;
    if (!failure || !INLINE_ERRORS.includes(failure.kind)) return;
    notify(
      "error",
      failure.kind === "conflict" && failure.message.includes("stash")
        ? "Applying the stash conflicted. The files hold the conflict markers and the stash is kept."
        : failure.message,
    );
  }

  const askStash = (tab: RepoTab) => openDialog(tab, { type: "stash" });
  const askDropStash = (tab: RepoTab, stash: StashEntry) =>
    openDialog(tab, { type: "dropStash", stash });
  const askReset = (tab: RepoTab, target: CommitTarget) =>
    openDialog(tab, { type: "reset", target });

  async function stashChanges(
    tab: RepoTab,
    message: string,
    includeUntracked: boolean,
    keepIndex: boolean,
  ): Promise<void> {
    const status = await write(tab, () =>
      backend.stashPush(tab.id, message.trim() || null, includeUntracked, keepIndex),
    );
    if (!status) return;
    applyStatus(tab, status, true);
    closeDialog(tab);
    void loadStashes(tab);
    notify("info", "Your changes are in the stash.");
  }

  /** Applies a stash; `pop` also removes it from the list once it was applied. */
  async function applyStash(tab: RepoTab, stash: StashEntry, pop: boolean): Promise<void> {
    const status = await write(tab, () =>
      pop
        ? backend.stashPop(tab.id, stash.reference, stash.id, false)
        : backend.stashApply(tab.id, stash.reference, stash.id, false),
    );
    void loadStashes(tab);
    if (!status) {
      reportInline(tab);
      void refreshStatus(tab, true);
      return;
    }
    applyStatus(tab, status, true);
    notify("info", pop ? "Stash applied and removed from the list." : "Stash applied.");
  }

  async function dropStash(tab: RepoTab, stash: StashEntry): Promise<void> {
    const dropped = await write(tab, () => backend.stashDrop(tab.id, stash.reference, stash.id));
    void loadStashes(tab);
    if (!dropped) return;
    closeDialog(tab);
    applyStatus(tab, dropped.status);
    notify("info", "Stash dropped.", {
      label: "Undo",
      run: async () => {
        const status = await write(tab, () =>
          backend.stashRestore(tab.id, dropped.id, dropped.message),
        );
        if (status) applyStatus(tab, status);
        void loadStashes(tab);
        if (status) notify("info", "The stash is back in the list.");
      },
    });
  }

  async function takeBackReplay(tab: RepoTab, before: string, after: string): Promise<void> {
    const status = await write(tab, () => backend.undoReplay(tab.id, before, after));
    if (status) {
      applyStatus(tab, status, true);
      notify("info", "Undone.");
    }
  }

  /** Copies a commit onto the current branch (a conflict cancels it and changes nothing). */
  async function cherryPick(tab: RepoTab, commit: CommitTarget): Promise<void> {
    const result = await write(tab, () => backend.cherryPick(tab.id, commit.id));
    if (!result) return reportInline(tab);
    applyStatus(tab, result.status, true);
    notify("info", `Cherry-picked ${commit.label}.`, {
      label: "Undo",
      run: () => takeBackReplay(tab, result.before, result.after),
    });
  }

  /** Adds a commit that undoes another one. */
  async function revert(tab: RepoTab, commit: CommitTarget): Promise<void> {
    const result = await write(tab, () => backend.revert(tab.id, commit.id));
    if (!result) return reportInline(tab);
    applyStatus(tab, result.status, true);
    notify("info", `Reverted ${commit.label}.`, {
      label: "Undo",
      run: () => takeBackReplay(tab, result.before, result.after),
    });
  }

  async function reset(tab: RepoTab, target: CommitTarget, mode: ResetMode): Promise<void> {
    const result = await write(tab, () => backend.reset(tab.id, target.id, mode));
    if (!result) return reportInline(tab);
    closeDialog(tab);
    applyStatus(tab, result.status, true);
    const snapshot = result.snapshot;
    const discarded = snapshot ? " Your uncommitted changes were discarded." : "";
    notify("info", `Moved ${tab.headBranch ?? "HEAD"} to ${target.label} (${mode}).${discarded}`, {
      label: "Undo",
      run: async () => {
        const undone = await write(tab, () =>
          backend.undoReset(tab.id, result.before, result.after, snapshot?.reference ?? null),
        );
        if (undone) {
          applyStatus(tab, undone.status, true);
          notify("info", "The branch is back where it was.");
        }
      },
    });
  }

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
        dialog: null,
        remote: null,
        stashes: [],
      });
      tabs.value = [...tabs.value, tab];
      activeId.value = tab.id;
      recentRepos.value = recents.add({ path: info.path, name: info.name, openedAt: Date.now() });
      if (tab.readOnly) void backend.setReadOnly(tab.id, true).catch(() => {});
      void refreshStatus(tab);
      void loadStashes(tab);
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
    askCreateBranch,
    askRenameBranch,
    askDeleteBranch,
    askCreateTag,
    askDeleteTag,
    askUpstream,
    askPush,
    askStash,
    askDropStash,
    askReset,
    loadStashes,
    stashChanges,
    applyStash,
    dropStash,
    cherryPick,
    revert,
    reset,
    askDeleteRemoteBranch,
    fetchRemote,
    pull,
    push,
    deleteRemoteBranch,
    cancelRemote,
    answerPrompt,
    closeDialog,
    checkout,
    createBranch,
    renameBranch,
    deleteBranch,
    setUpstream,
    createTag,
    deleteTag,
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
