import { computed, ref, shallowReactive, shallowRef, watch } from "vue";
import {
  type ActivityEntry,
  type AuthorCount,
  type AutoFetchResult,
  type BranchPr,
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
  type MergeInfo,
  type MergeMode,
  type ReflogEntry,
  type Side,
  type PullStrategy,
  type RebasePlan,
  type TodoItem,
  type Worktree,
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
import { dueForFetch } from "@/lib/autoFetch";
import { folderName, freeBranches, samePath, worktreeOfRefusal } from "@/lib/worktrees";
import { personKey } from "@/lib/metro/people";
import { createReadOnlyStore, type ReadOnlyStore } from "@/lib/readOnly";
import { createRecentStore, type RecentRepo, type RecentStore } from "@/lib/recents";
import {
  createPullRequestUrl,
  type HostedRepo,
  pickHostedRepo,
  pullRequestUrl,
} from "@/lib/remoteUrl";
import { pullRequestDescription, pullRequestTitle } from "@/lib/pullRequestText";
import { ticketFromBranch, ticketPrefix, ticketUrl } from "@/lib/tickets";
import { createSessionStore, type Session, type SessionStore } from "@/lib/session";

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
  /** A new working tree: the local branches free to take; `error` is what git said. */
  | { type: "createWorktree"; branches: string[]; branch: string | null; error: string | null }
  /** Remove a working tree; `error` is git's refusal (changes there), which `force` overrides. */
  | { type: "removeWorktree"; worktree: Worktree; error: string | null }
  /** Create a pull request from `branch`; `error` is what the host or the push said. */
  | {
      type: "createPullRequest";
      branch: string;
      remote: string;
      targets: string[];
      target: string;
      title: string;
      description: string;
      /** The branch is not on the remote, or the remote is at another commit. */
      needsPush: boolean;
      /** The branch has no counterpart on the remote yet, so the push makes it the upstream. */
      newOnRemote: boolean;
      error: string | null;
    }
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
  | { type: "reset"; target: CommitTarget }
  /** Merge a branch into the current one; `info` is what the merge would do (when known). */
  | { type: "merge"; target: string; info: MergeInfo | null }
  /** Cancel the merge, cherry-pick, revert or rebase in progress: the work done so far is lost. */
  | { type: "abortOperation" }
  /** Rebase the current branch onto `onto`; `plan` lists what would be replayed (when known). */
  | { type: "rebase"; onto: string; plan: RebasePlan | null; interactive: boolean };

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
  /** The positions HEAD went through, for the history panel. */
  reflog: ReflogEntry[];
  /** The hosting service of the first known remote, for ticket links. */
  hosted: HostedRepo | null;
  /** The open pull requests of the repository, by source branch (for the badges). */
  pullRequests: BranchPr[];
  /** The last refresh of the pull requests failed: what is shown may be out of date. */
  prStale: boolean;
  prLoading: boolean;
  /** The user is not signed in with the tool of the host: nothing can be asked of it. */
  prSignedOut: boolean;
  /** The authors of the repository, for the focus selector. */
  authors: AuthorCount[];
  /** The working trees of the repository (the main one first), for the sidebar. */
  worktrees: Worktree[];
  /** When the last automatic fetch ended (milliseconds since the Unix epoch, 0: never). */
  autoFetchedAt: number;
  autoFetching: boolean;
  /** The last automatic fetch failed (it never asks questions: fetch by hand to see why). */
  autoFetchFailed: boolean;
  /** Commits waiting to be pulled over the branches that have an upstream, as of the last fetch. */
  incoming: number;
  /** The author the graph is focused on (`personKey` of the name), `null` for everyone. */
  focusAuthor: string | null;
  /** When the pull requests were last asked for (milliseconds since the Unix epoch). */
  prLoadedAt: number;
  activity: ActivityEntry[];
  /** The branch or tag dialog being shown, if any. */
  dialog: BranchDialog | null;
  /** The fetch, pull or push in progress, if any. */
  remote: RemoteOp | null;
  /** The stash list, most recent first. */
  stashes: StashEntry[];
  /** How many times the speed of `git status` was looked at (it is offered once at most). */
  speedChecks: number;
  /** The tip of the branch before a rebase that is still going on, for Undo once it ends. */
  rebaseBefore: string | null;
  /** The conflict resolver is open on this file (`""` before a file is chosen); `null` if closed. */
  resolverPath: string | null;
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
  sessionStore: SessionStore = createSessionStore(null),
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

  // ---------- session: the tabs come back at the next start ----------

  /** Set while the tabs of the last session are reopened, so a partial state is not saved. */
  let restoring = false;

  const currentSession = (): Session => ({
    paths: tabs.value.map((tab) => tab.path),
    active: active.value?.path ?? null,
    hidden: Object.fromEntries(
      tabs.value.filter((tab) => tab.hidden.size > 0).map((tab) => [tab.path, [...tab.hidden]]),
    ),
  });

  watch(
    () => JSON.stringify(currentSession()),
    () => {
      if (!restoring) sessionStore.write(currentSession());
    },
    { flush: "sync" },
  );

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
    void loadWorktrees(tab);
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
      // The branch checked out changes with a checkout, a rename or a deletion.
      tab.headBranch = refs.find((ref) => ref.kind === "local" && ref.isHead)?.name ?? null;
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
  const historyOpen = ref(false);

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
          void offerSpeedUp(tab);
        } catch {
          // A repository without a working tree, or a transient failure: keep what we have.
        }
      } while (tab.statusQueued && tabs.value.includes(tab));
    } finally {
      tab.statusRefreshing = false;
    }
  }

  /** On a large working tree a slow status is worth fixing: offer, once, to switch on git's cache. */
  async function offerSpeedUp(tab: RepoTab): Promise<void> {
    if (tab.speedChecks >= 3) return;
    tab.speedChecks++;
    try {
      const hint = await backend.statusHint(tab.id);
      if (!hint.canSpeedUp) return;
      tab.speedChecks = 3;
      notify(
        "info",
        `Reading the changes takes ${(hint.lastMs / 1000).toFixed(1)} s in this large repository. Git can remember its folder scan to make it much faster; that adds one setting (core.untrackedCache) to this repository.`,
        {
          label: "Turn it on",
          run: async () => {
            const faster = await write(tab, () => backend.speedUpStatus(tab.id));
            if (faster) {
              notify("info", `Done: the status now takes ${(faster.lastMs / 1000).toFixed(1)} s.`);
            }
          },
        },
      );
    } catch {
      // Not worth bothering the user with.
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
    if (failure.kind === "locked") return; // `write` offers to retry
    if (!INLINE_ERRORS.includes(failure.kind)) notify("error", friendly(failure));
  }

  /** What git says, with a plain-language first sentence for the failures people meet often. */
  function friendly(failure: { kind: GitErrorKind; message: string }): string {
    switch (failure.kind) {
      case "authFailed":
        return `The remote refused the credentials. Check your login, token or SSH key. (${failure.message})`;
      case "unreachable":
        return `Cannot reach the remote. Check the network and the address of the remote. (${failure.message})`;
      default:
        return failure.message;
    }
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
      if (toGitError(error).kind === "locked") {
        // Another git process holds a lock: never remove it, offer to try again.
        notify(
          "error",
          "Another git process is working in this repository (a lock file exists). Wait for it to finish, then try again. The app never deletes lock files.",
          { label: "Try again", run: () => void write(tab, action) },
        );
      }
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

  const stageLines = (tab: RepoTab, path: string, hunkId: string, lines: number[]) =>
    writeStatus(tab, () => backend.stageLines(tab.id, path, hunkId, lines));
  const unstageLines = (tab: RepoTab, path: string, hunkId: string, lines: number[]) =>
    writeStatus(tab, () => backend.unstageLines(tab.id, path, hunkId, lines));

  /** Who wrote each line of a file, as of a commit (`null`: the working tree). */
  const blame = (tab: RepoTab, commit: string | null, path: string) =>
    backend.blame(tab.id, commit, path);

  /** The row of a commit in the graph, `null` when it is not drawn. */
  async function locateCommit(tab: RepoTab, id: string): Promise<number | null> {
    const found = await backend.locate(tab.id, [id]).catch(() => [null]);
    return found[0] ?? null;
  }

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

  /** What the last prefill wrote, per tab: it can be replaced when the branch changes. */
  const prefilled = new Map<string, string>();
  /** Whether an empty commit box starts with the ticket of the branch. */
  const ticketPrefill = ref(true);

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
    prefilled.delete(tab.id);
    prefillTicket(tab);
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
      const elsewhere = failure ? worktreeOfRefusal(failure.message) : null;
      if (elsewhere) {
        // The branch is open in another working tree: say so, and offer to go there.
        tab.writeError = null;
        notify(
          "error",
          `${targetLabel(target)} is checked out in another working tree (${folderName(elsewhere)}), so it cannot be checked out here.`,
          { label: "Open it", run: () => openPath(elsewhere) },
        );
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
    if (remote === null) {
      const remotes = await backend.remotes(tab.id).catch(() => null);
      if (remotes !== null && remotes.length === 0) {
        notify("info", "This repository has no remote to fetch from.");
        return;
      }
    }
    const result = await runRemote(tab, "fetch", "Fetching…", (opId) =>
      backend.fetch(tab.id, opId, remote, prune),
    );
    if (result) {
      notify(
        "info",
        result.summary.trim() ? "Fetched the remotes." : "Nothing new on the remotes.",
      );
      void loadPullRequests(tab, true);
      void autoFetch(tab);
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
      void autoFetch(tab);
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
        "The pull stopped on conflicts: resolve them from the banner above the graph.",
      );
      void refreshStatus(tab, true);
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

  // ---------- merge and conflicts ----------

  const OPERATION_NAMES = {
    merge: "merge",
    rebase: "rebase",
    cherryPick: "cherry-pick",
    revert: "revert",
  } as const;

  function openResolver(tab: RepoTab, path?: string): void {
    const first = tab.status?.conflicted[0]?.path ?? "";
    tab.resolverPath = path ?? first;
  }

  const closeResolver = (tab: RepoTab): void => {
    tab.resolverPath = null;
  };

  /** After a file was settled: go to the next one, or close the resolver when none is left. */
  function afterResolved(tab: RepoTab, status: WorkingStatus): void {
    applyStatus(tab, status, true);
    if (status.conflicted.length > 0) {
      tab.resolverPath = status.conflicted[0].path;
    } else {
      closeResolver(tab);
      notify(
        "info",
        status.operation
          ? `Every conflict is resolved. Continue the ${OPERATION_NAMES[status.operation.kind]} when you are ready.`
          : "Every conflict is resolved.",
      );
    }
  }

  async function askMerge(tab: RepoTab, target: string): Promise<void> {
    const info = await backend.mergeInfo(tab.id, target).catch(() => null);
    openDialog(tab, { type: "merge", target, info });
  }

  async function merge(
    tab: RepoTab,
    target: string,
    mode: MergeMode,
    message: string,
  ): Promise<void> {
    const result = await write(tab, () =>
      backend.merge(tab.id, target, mode, message.trim() || null),
    );
    if (!result) {
      const failure = tab.writeError;
      if (failure && INLINE_ERRORS.includes(failure.kind)) notify("error", failure.message);
      return;
    }
    closeDialog(tab);
    applyStatus(tab, result.status, true);
    const undo = {
      label: "Undo",
      run: async () => {
        const status = await write(tab, () =>
          backend.undoReplay(tab.id, result.before, result.after),
        );
        if (status) {
          applyStatus(tab, status, true);
          notify("info", "Merge undone.");
        }
      },
    };
    switch (result.outcome) {
      case "upToDate":
        notify("info", `${target} is already part of this branch: nothing to merge.`);
        break;
      case "fastForward":
        notify("info", `Fast-forwarded to ${target}.`, undo);
        break;
      case "merged":
        notify("info", `Merged ${target}.`, undo);
        break;
      case "squashed":
        notify("info", `The changes of ${target} are staged. Commit them when you are ready.`);
        break;
      case "conflicts":
        notify(
          "error",
          `Merging ${target} stopped on conflicts: resolve them, then continue (or abort the merge).`,
        );
        openResolver(tab);
        break;
    }
  }

  async function resolveFile(tab: RepoTab, path: string, content: string): Promise<boolean> {
    const status = await write(tab, () => backend.resolveFile(tab.id, path, content));
    if (!status) return false;
    afterResolved(tab, status);
    return true;
  }

  async function resolveSide(tab: RepoTab, path: string, side: Side): Promise<boolean> {
    const status = await write(tab, () => backend.resolveSide(tab.id, path, side));
    if (!status) return false;
    afterResolved(tab, status);
    return true;
  }

  async function continueOperation(tab: RepoTab): Promise<void> {
    const wasRebase = tab.status?.operation?.kind === "rebase";
    const name = tab.status?.operation ? OPERATION_NAMES[tab.status.operation.kind] : "operation";
    const status = await write(tab, () => backend.continueOperation(tab.id));
    if (!status) {
      const failure = tab.writeError;
      if (failure && INLINE_ERRORS.includes(failure.kind)) notify("error", failure.message);
      return;
    }
    applyStatus(tab, status, true);
    if (status.conflicted.length > 0) {
      openResolver(tab);
      notify("error", `Continued the ${name}: the next commit conflicts.`);
      return;
    }
    closeResolver(tab);
    if (status.operation) {
      notify("info", `Continued the ${name}: it stopped again.`);
      return;
    }
    const before = wasRebase ? tab.rebaseBefore : null;
    const after = status.head;
    tab.rebaseBefore = null;
    notify(
      "info",
      `The ${name} is finished.`,
      before && after
        ? {
            label: "Undo",
            run: async () => {
              const undone = await write(tab, () => backend.undoReplay(tab.id, before, after));
              if (undone) {
                applyStatus(tab, undone, true);
                notify("info", "Rebase undone.");
              }
            },
          }
        : undefined,
    );
  }

  const askAbortOperation = (tab: RepoTab) => openDialog(tab, { type: "abortOperation" });

  async function abortOperation(tab: RepoTab): Promise<void> {
    const status = await write(tab, () => backend.abortOperation(tab.id));
    if (!status) return;
    closeDialog(tab);
    closeResolver(tab);
    tab.rebaseBefore = null;
    applyStatus(tab, status, true);
    notify("info", "Cancelled: the repository is back where it was.");
  }

  // ---------- rebase ----------

  /** Asks how to rebase the current branch onto `onto`, once the commits it would replay are known. */
  async function askRebase(tab: RepoTab, onto: string, interactive = false): Promise<void> {
    const plan = await backend.rebasePlan(tab.id, onto).catch(() => null);
    openDialog(tab, { type: "rebase", onto, plan, interactive });
  }

  async function rebase(
    tab: RepoTab,
    onto: string,
    items: TodoItem[] | null,
    autostash: boolean,
  ): Promise<void> {
    const result = await write(tab, () => backend.rebase(tab.id, onto, items, autostash));
    if (!result) {
      // The rebase may have stopped on something other than a conflict: the banner must show it.
      void refreshStatus(tab, true);
      const failure = tab.writeError;
      if (failure?.kind === "wouldOverwriteChanges") {
        notify(
          "error",
          "You have uncommitted changes. Commit or stash them first, or tick “Stash my changes while rebasing”.",
        );
      } else if (failure && INLINE_ERRORS.includes(failure.kind)) {
        notify("error", failure.message);
      }
      return;
    }
    closeDialog(tab);
    applyStatus(tab, result.status, true);
    switch (result.outcome) {
      case "upToDate":
        notify("info", `This branch is already on top of ${onto}.`);
        break;
      case "done":
        notify("info", `Rebased onto ${onto}.`, {
          label: "Undo",
          run: async () => {
            const status = await write(tab, () =>
              backend.undoReplay(tab.id, result.before, result.after),
            );
            if (status) {
              applyStatus(tab, status, true);
              notify("info", "Rebase undone.");
            }
          },
        });
        break;
      case "stopped":
        tab.rebaseBefore = result.before;
        notify(
          "info",
          "The rebase stopped for you to edit a commit. Make your changes, commit or amend, then continue.",
        );
        break;
      case "conflicts":
        tab.rebaseBefore = result.before;
        notify(
          "error",
          `The rebase stopped on conflicts: resolve them, then continue (or skip the commit, or abort).`,
        );
        openResolver(tab);
        break;
    }
  }

  async function skipOperation(tab: RepoTab): Promise<void> {
    const status = await write(tab, () => backend.skipOperation(tab.id));
    if (!status) {
      const failure = tab.writeError;
      if (failure && INLINE_ERRORS.includes(failure.kind)) notify("error", failure.message);
      return;
    }
    applyStatus(tab, status, true);
    if (status.conflicted.length > 0) {
      openResolver(tab);
      notify("info", "Skipped that commit. The next one conflicts too.");
    } else {
      closeResolver(tab);
      notify(
        "info",
        status.operation ? "Skipped that commit." : "Skipped. The operation is finished.",
      );
    }
  }

  async function conflictFile(tab: RepoTab, path: string) {
    return backend.conflictFile(tab.id, path);
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

  async function loadReflog(tab: RepoTab): Promise<void> {
    try {
      tab.reflog = await backend.reflog(tab.id);
    } catch {
      tab.reflog = [];
    }
  }

  /** Takes back the last operation that moved HEAD; the notice offers to take it back again. */
  async function undoLast(tab: RepoTab): Promise<void> {
    const result = await write(tab, () => backend.undoLast(tab.id));
    if (!result) return reportInline(tab);
    applyStatus(tab, result.status, true);
    notify("info", `Undid ${result.undone}.`, { label: "Redo", run: () => undoLast(tab) });
    if (historyOpen.value) void loadReflog(tab);
  }

  /** Puts the branch back where it was at a point of the history. */
  async function restoreHead(tab: RepoTab, entry: ReflogEntry): Promise<void> {
    const status = await write(tab, () => backend.restoreHead(tab.id, entry.id));
    if (!status) return reportInline(tab);
    applyStatus(tab, status, true);
    notify("info", `Moved ${tab.headBranch ?? "HEAD"} back to ${entry.id.slice(0, 7)}.`);
    void loadReflog(tab);
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

  async function openPath(path: string, hidden: readonly string[] = []): Promise<void> {
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
        name: info.isLinked && info.headBranch ? `${info.name} (${info.headBranch})` : info.name,
        path: info.path,
        headBranch: info.headBranch,
        refs,
        hidden: new Set(hidden),
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
        reflog: [],
        hosted: null,
        pullRequests: [],
        prStale: false,
        prLoading: false,
        prSignedOut: false,
        authors: [],
        worktrees: [],
        autoFetchedAt: 0,
        autoFetching: false,
        autoFetchFailed: false,
        incoming: 0,
        focusAuthor: null,
        prLoadedAt: 0,
        activity: [],
        dialog: null,
        remote: null,
        stashes: [],
        resolverPath: null,
        rebaseBefore: null,
        speedChecks: 0,
      });
      tabs.value = [...tabs.value, tab];
      activeId.value = tab.id;
      recentRepos.value = recents.add({ path: info.path, name: info.name, openedAt: Date.now() });
      if (tab.readOnly) void backend.setReadOnly(tab.id, true).catch(() => {});
      void refreshStatus(tab);
      void loadStashes(tab);
      void loadWorktrees(tab);
      await loadGraph(tab);
    } catch (error) {
      openError.value = describeOpenError(trimmed, error);
    } finally {
      opening.value = false;
    }
  }

  /**
   * Reopens the tabs of the last session, with the branches they hid, and shows the tab that
   * was shown. A repository that cannot be opened any more is left out, with one notice.
   */
  async function restoreSession(): Promise<void> {
    const session = sessionStore.read();
    if (session.paths.length === 0) return;
    restoring = true;
    const missing: string[] = [];
    try {
      for (const path of session.paths) {
        await openPath(path, session.hidden[path] ?? []);
        if (openError.value) missing.push(path.split("/").filter(Boolean).pop() ?? path);
      }
      openError.value = "";
      const shown = tabs.value.find((tab) => tab.path === session.active);
      if (shown) activeId.value = shown.id;
    } finally {
      restoring = false;
      sessionStore.write(currentSession());
    }
    if (missing.length > 0) {
      notify(
        "error",
        `${missing.length === 1 ? "1 repository" : `${missing.length} repositories`} could not be reopened: ${missing.join(", ")}.`,
      );
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

  /** Hides a group of refs (a folder of the sidebar), or shows them when all are hidden. */
  function toggleRefs(tab: RepoTab, fullNames: string[]): Promise<void> {
    if (fullNames.length === 0) return Promise.resolve();
    return setHidden(tab, fullNames, !fullNames.every((name) => tab.hidden.has(name)));
  }

  /** Hides a whole kind of refs, or shows it again when everything of that kind is hidden. */
  function toggleKind(tab: RepoTab, kind: RefKind): Promise<void> {
    const names = tab.refs.filter((ref) => ref.kind === kind).map((ref) => ref.fullName);
    const allHidden = names.every((name) => tab.hidden.has(name));
    return setHidden(tab, names, !allHidden);
  }

  /**
   * Fetches in the background, never asking a question. A failure is only marked on the tab.
   * Returns what the fetch brought, `null` when it did not run or failed.
   */
  async function autoFetch(tab: RepoTab): Promise<AutoFetchResult | null> {
    if (tab.autoFetching || tab.remote !== null) return null;
    tab.autoFetching = true;
    try {
      const result = await backend.autoFetch(tab.id);
      tab.incoming = result.behindTotal;
      tab.autoFetchFailed = false;
      if (result.updates.length > 0) void refreshGraph(tab);
      return result;
    } catch {
      tab.autoFetchFailed = true;
      return null;
    } finally {
      tab.autoFetching = false;
      tab.autoFetchedAt = Date.now();
    }
  }

  /** The repositories an automatic fetch is due for. */
  function autoFetchDue(minutes: number, now = Date.now()): RepoTab[] {
    return dueForFetch(
      tabs.value.map((tab) => ({
        tab,
        autoFetchedAt: tab.autoFetchedAt,
        autoFetching: tab.autoFetching,
        remoteBusy: tab.remote !== null,
        busy: tab.busy,
      })),
      minutes,
      now,
    ).map((entry) => entry.tab);
  }

  async function loadWorktrees(tab: RepoTab): Promise<void> {
    try {
      tab.worktrees = await backend.worktrees(tab.id);
    } catch {
      tab.worktrees = [];
    }
  }

  /** Opens a working tree in its own tab (or shows the one that is open). */
  const openWorktree = (tree: Worktree): Promise<void> => openPath(tree.path);

  /**
   * Opens a working tree in the editor or the terminal. These act on a repository, so the working
   * tree is opened in its own tab first (which then shows).
   */
  async function openWorktreeIn(
    tree: Worktree,
    kind: "editor" | "terminal",
    app: string,
  ): Promise<void> {
    await openPath(tree.path);
    const opened = tabs.value.find((tab) => samePath(tab.path, tree.path));
    if (!opened) return;
    if (kind === "editor") await openInEditor(opened, app, null);
    else await openInTerminal(opened, app);
  }

  /** Asks where to put a new working tree, for `branch` when given. */
  function askCreateWorktree(tab: RepoTab, branch: string | null = null): void {
    tab.writeError = null;
    tab.dialog = {
      type: "createWorktree",
      branches: freeBranches(tab.refs, tab.worktrees),
      branch,
      error: null,
    };
  }

  /**
   * Creates the working tree and, when asked, opens it. A refusal of git stays in the dialog.
   * `branch` is existing unless `newBranch`, which creates it from `start` (HEAD when empty).
   */
  async function createWorktree(
    tab: RepoTab,
    form: { path: string; branch: string; newBranch: boolean; start: string; open: boolean },
  ): Promise<void> {
    const dialog = tab.dialog;
    if (dialog?.type !== "createWorktree" || tab.busy) return;
    tab.busy = true;
    try {
      await backend.addWorktree(
        tab.id,
        form.path.trim(),
        form.branch.trim(),
        form.newBranch,
        form.newBranch && form.start.trim() ? form.start.trim() : null,
      );
    } catch (error) {
      if (tab.dialog === dialog) tab.dialog = { ...dialog, error: message(error) };
      return;
    } finally {
      tab.busy = false;
    }
    closeDialog(tab);
    notify("info", `Created the working tree ${folderName(form.path)}.`);
    await loadWorktrees(tab);
    if (form.open) await openPath(form.path.trim());
  }

  function askRemoveWorktree(tab: RepoTab, tree: Worktree): void {
    if (tree.isMain) return;
    tab.writeError = null;
    tab.dialog = { type: "removeWorktree", worktree: tree, error: null };
  }

  /** Removes the folder with `git worktree remove`; changes there make git refuse unless `force`. */
  async function removeWorktree(tab: RepoTab, tree: Worktree, force: boolean): Promise<void> {
    const dialog = tab.dialog;
    if (tab.busy) return;
    tab.busy = true;
    try {
      await backend.removeWorktree(tab.id, tree.path, force);
    } catch (error) {
      if (dialog?.type === "removeWorktree" && tab.dialog === dialog) {
        tab.dialog = { ...dialog, error: message(error) };
      } else {
        notify("error", message(error));
      }
      return;
    } finally {
      tab.busy = false;
    }
    closeDialog(tab);
    // A tab open on the removed folder has nothing left to show.
    const open = tabs.value.find((other) => samePath(other.path, tree.path));
    if (open) void closeTab(open.id);
    notify("info", `Removed the working tree ${folderName(tree.path)}.`);
    await loadWorktrees(tab);
  }

  async function toggleLockWorktree(tab: RepoTab, tree: Worktree): Promise<void> {
    try {
      await backend.lockWorktree(tab.id, tree.path, tree.locked === null);
    } catch (error) {
      notify("error", message(error));
    }
    await loadWorktrees(tab);
  }

  async function pruneWorktrees(tab: RepoTab): Promise<void> {
    try {
      await backend.pruneWorktrees(tab.id);
      notify("info", "Forgot the working trees whose folder is gone.");
    } catch (error) {
      notify("error", message(error));
    }
    await loadWorktrees(tab);
  }

  async function loadAuthors(tab: RepoTab): Promise<void> {
    try {
      tab.authors = await backend.authors(tab.id);
    } catch {
      tab.authors = [];
    }
  }

  /** Fades everything but the commits of an author (`null` brings everyone back). */
  function setFocusAuthor(tab: RepoTab, author: string | null): void {
    tab.focusAuthor = author === null || author === "" ? null : personKey(author);
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

  // ---------- outside the application: pull requests, editor, terminal ----------

  /**
   * Opens the pull request of a branch in the browser: the open one when the host says there is
   * one (it needs a token), otherwise the page that starts one. A failed lookup is reported and
   * the page that starts one is opened anyway.
   */
  async function openPullRequest(tab: RepoTab, branch: string): Promise<void> {
    const remotes = await backend.remoteUrls(tab.id).catch(() => []);
    const hosted = pickHostedRepo(remotes);
    if (!hosted) {
      notify(
        "error",
        "No remote of this repository is on GitHub or Azure DevOps: there is no pull request page to open.",
      );
      return;
    }
    let url = createPullRequestUrl(hosted.repo, branch);
    try {
      const lookup = await backend.findPullRequest(hosted.repo, branch);
      if (lookup.state === "found") {
        url = pullRequestUrl(hosted.repo, lookup.id);
        notify("info", `Pull request ${lookup.id}: ${lookup.title}`);
      }
    } catch (error) {
      notify("error", `${message(error)} Opening the page to create one instead.`);
    }
    try {
      await backend.openExternal(url);
    } catch (error) {
      notify("error", message(error));
    }
  }

  /** Reads which hosting service the remotes are on (ticket links need it). */
  async function loadHosted(tab: RepoTab): Promise<void> {
    const remotes = await backend.remoteUrls(tab.id).catch(() => []);
    tab.hosted = pickHostedRepo(remotes)?.repo ?? null;
  }

  /** One notice per session for a failed read of the pull requests, so a bad network is not noisy. */
  let prErrorShown = false;

  /**
   * Reads the open pull requests with their review and build state. Asked at most every 30
   * seconds unless `force`; without a sign-in nothing is shown; a failure keeps what was
   * shown, marked as out of date.
   */
  async function loadPullRequests(tab: RepoTab, force = false): Promise<void> {
    if (tab.prLoading || (!force && Date.now() - tab.prLoadedAt < 30_000)) return;
    if (!tab.hosted) await loadHosted(tab);
    const hosted = tab.hosted;
    if (!hosted) {
      tab.pullRequests = [];
      return;
    }
    tab.prLoading = true;
    try {
      const list = await backend.listPullRequests(hosted);
      tab.pullRequests = list.state === "listed" ? list.items : [];
      tab.prSignedOut = list.state === "signedOut";
      tab.prStale = false;
    } catch (error) {
      tab.prStale = tab.pullRequests.length > 0;
      if (!prErrorShown) {
        prErrorShown = true;
        notify("error", `Pull request status: ${message(error)}`);
      }
    } finally {
      tab.prLoading = false;
      tab.prLoadedAt = Date.now();
    }
  }

  /**
   * What "pull request" does for a branch: open the one that exists; otherwise ask for the text
   * of a new one and create it here (the user is signed in with the tool of the host), or open the
   * page of the host that starts one.
   */
  async function pullRequestAction(tab: RepoTab, branch: string): Promise<void> {
    if (!tab.hosted) await loadHosted(tab);
    if (tab.prLoadedAt === 0) await loadPullRequests(tab);
    const existing = tab.pullRequests.find((pr) => pr.branch === branch);
    if (existing) return openPullRequestPage(tab, existing.id);
    if (!tab.hosted || tab.prSignedOut || tab.readOnly) return openPullRequest(tab, branch);
    return askCreatePullRequest(tab, branch, tab.hosted);
  }

  /** Opens the dialog of a new pull request with the target, the title and the text filled in. */
  async function askCreatePullRequest(
    tab: RepoTab,
    branch: string,
    hosted: HostedRepo,
  ): Promise<void> {
    const remotes = await backend.remoteUrls(tab.id).catch(() => []);
    const remote = pickHostedRepo(remotes)?.remote ?? remotes[0]?.name ?? "origin";
    const prefix = `${remote}/`;
    const targets = tab.refs
      .filter((ref) => ref.kind === "remote" && ref.name.startsWith(prefix))
      .map((ref) => ref.name.slice(prefix.length))
      .filter((name) => name !== "HEAD" && name !== branch);
    const asked = await backend.defaultBranch(hosted).catch(() => null);
    const target =
      [asked, "main", "master", "develop"].find(
        (name): name is string => !!name && name !== branch && targets.includes(name),
      ) ??
      targets[0] ??
      asked ??
      "main";
    const commits = await backend
      .commitsBetween(tab.id, `${prefix}${target}`, branch)
      .catch(() => []);
    const local = tab.refs.find((ref) => ref.kind === "local" && ref.name === branch);
    const onRemote = tab.refs.find(
      (ref) => ref.kind === "remote" && ref.name === `${prefix}${branch}`,
    );
    tab.writeError = null;
    tab.dialog = {
      type: "createPullRequest",
      branch,
      remote,
      targets: targets.includes(target) ? targets : [target, ...targets],
      target,
      title: pullRequestTitle(branch, commits, hosted),
      description: pullRequestDescription(branch, commits, hosted),
      needsPush: !onRemote || (!!local && onRemote.target !== local.target),
      newOnRemote: !onRemote,
      error: null,
    };
  }

  /**
   * Creates the pull request, pushing the branch first when asked. A refusal (of the push or of
   * the host) stays in the dialog so the text is not lost.
   */
  async function createPullRequest(
    tab: RepoTab,
    form: { target: string; title: string; description: string; draft: boolean; push: boolean },
  ): Promise<void> {
    const dialog = tab.dialog;
    const hosted = tab.hosted;
    if (dialog?.type !== "createPullRequest" || !hosted || tab.busy) return;
    const fail = (error: string) => {
      if (tab.dialog === dialog) tab.dialog = { ...dialog, error };
    };
    if (form.push) {
      const pushed = await runRemote(tab, "push", `Pushing ${dialog.branch}…`, (opId) =>
        backend.push(tab.id, opId, {
          remote: dialog.remote,
          branch: dialog.branch,
          setUpstream: dialog.newOnRemote,
          forceWithLease: false,
        }),
      );
      if (!pushed) {
        const failure = tab.writeError;
        tab.writeError = null;
        return fail(`The push failed: ${failure?.message ?? "see the activity log."}`);
      }
    }
    tab.busy = true;
    try {
      const id = await backend.createPullRequest(hosted, {
        source: dialog.branch,
        target: form.target,
        title: form.title.trim(),
        description: form.description.trim(),
        draft: form.draft,
      });
      closeDialog(tab);
      notify("info", `Created pull request ${id}.`);
      void loadPullRequests(tab, true);
      await backend.openExternal(pullRequestUrl(hosted, id)).catch(() => {});
    } catch (error) {
      fail(message(error));
    } finally {
      tab.busy = false;
    }
  }

  /** Opens a pull request, from its number, in the browser. */
  async function openPullRequestPage(tab: RepoTab, id: number): Promise<void> {
    if (!tab.hosted) return;
    try {
      await backend.openExternal(pullRequestUrl(tab.hosted, id));
    } catch (error) {
      notify("error", message(error));
    }
  }

  /** Opens a ticket of the hosting service in the browser. */
  async function openTicket(tab: RepoTab, ticket: string): Promise<void> {
    if (!tab.hosted) await loadHosted(tab);
    if (!tab.hosted) {
      notify("error", "No remote of this repository is on a known host: there is no ticket page.");
      return;
    }
    try {
      await backend.openExternal(ticketUrl(tab.hosted, ticket));
    } catch (error) {
      notify("error", message(error));
    }
  }

  /**
   * Starts the summary with the ticket of the branch (`AB#5310 `) when the box is empty. A text
   * the user wrote, an amend and a branch without a ticket are left alone.
   */
  function prefillTicket(tab: RepoTab): void {
    const previous = prefilled.get(tab.id);
    const { summary, body, amend } = tab.draft;
    if (!ticketPrefill.value || amend || !tab.hosted || !tab.headBranch) return;
    const ticket = ticketFromBranch(tab.headBranch);
    const untouched = summary.trim() === "" || summary === previous;
    if (!untouched || body.trim() !== "") return;
    if (!ticket) {
      if (previous !== undefined && summary === previous) updateDraft(tab, { summary: "" });
      prefilled.delete(tab.id);
      return;
    }
    const text = `${ticketPrefix(tab.hosted)}${ticket} `;
    prefilled.set(tab.id, text);
    if (summary !== text) updateDraft(tab, { summary: text });
  }

  /** Opens the repository, or one file of it, in the editor. */
  async function openInEditor(tab: RepoTab, editor: string, path: string | null): Promise<void> {
    try {
      await backend.openInEditor(tab.id, editor, path);
    } catch (error) {
      notify("error", message(error));
    }
  }

  /** Opens a terminal in the repository. */
  async function openInTerminal(tab: RepoTab, terminal: string): Promise<void> {
    try {
      await backend.openInTerminal(tab.id, terminal);
    } catch (error) {
      notify("error", message(error));
    }
  }

  function clearRecents(): void {
    recentRepos.value = recents.clear();
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
    askMerge,
    askRebase,
    rebase,
    skipOperation,
    merge,
    openResolver,
    closeResolver,
    resolveFile,
    resolveSide,
    conflictFile,
    continueOperation,
    askAbortOperation,
    abortOperation,
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
    historyOpen,
    loadReflog,
    undoLast,
    restoreHead,
    stageLines,
    unstageLines,
    blame,
    locateCommit,
    pickFolder: () => backend.pickFolder(),
    loadWorktrees,
    openWorktree,
    openWorktreeIn,
    askCreateWorktree,
    createWorktree,
    askRemoveWorktree,
    removeWorktree,
    toggleLockWorktree,
    pruneWorktrees,
    autoFetch,
    autoFetchDue,
    loadAuthors,
    setFocusAuthor,
    loadHosted,
    loadPullRequests,
    pullRequestAction,
    createPullRequest,
    openPullRequestPage,
    openTicket,
    prefillTicket,
    ticketPrefill,
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
    toggleRefs,
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
    clearRecents,
    restoreSession,
    openPullRequest,
    openInEditor,
    openInTerminal,
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
    shared = createWorkspace(
      backend,
      createRecentStore(storage),
      createReadOnlyStore(storage),
      createSessionStore(storage),
    );
  }
  return shared;
}
