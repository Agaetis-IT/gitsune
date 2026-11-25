import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  type ActivityEntry,
  type CommitOpResult,
  type DroppedStash,
  type ResetMode,
  type ResetResult,
  type StashEntry,
  cherryPick,
  listStashes,
  resetTo,
  revertCommit,
  stashApply,
  stashDrop,
  stashPop,
  stashPush,
  stashRestore,
  undoReplay,
  undoReset,
  type AskpassEvent,
  type PullStrategy,
  type PushOptions,
  type RemoteProgressEvent,
  type RemoteResult,
  type CheckoutResult,
  type CheckoutTarget,
  type DeletedBranch,
  type DeletedTag,
  type DirtyStrategy,
  checkoutTarget,
  createBranch,
  fetchRemote,
  pullBranch,
  pushBranch,
  deleteRemoteBranch,
  listRemotes,
  cancelOperation,
  answerAskpass,
  createTag,
  deleteBranch,
  deleteTag,
  renameBranch,
  restoreBranch,
  restoreTag,
  setUpstream,
  type Area,
  type CommitResult,
  type DiscardResult,
  type Identity,
  type Snapshot,
  type StatusEntry,
  type WorkingStatus,
  clearActivity,
  commitChanges,
  discardPaths,
  getActivity,
  getIdentity,
  getStatus,
  getWorkingDiff,
  listSnapshots,
  restoreSnapshot,
  setIdentity,
  setReadOnly,
  stageHunk,
  stagePaths,
  undoCommit,
  unstageHunk,
  unstagePaths,
  getCommitDetail,
  getFileDiff,
  getGraphMeta,
  type CommitDetail,
  type FileChange,
  type FileDiff,
  getGraphPage,
  locateCommits,
  searchCommits,
  type SearchResult,
  type GraphMeta,
  type GraphPage,
  listRefs,
  openRepo,
  closeRepo,
  type RefInfo,
  type RepoInfo,
} from "@/lib/api";

/** A graph ready to be drawn: its description and a way to fetch its rows. */
export interface GraphSource {
  meta: GraphMeta;
  fetchPage: (cursor: number, limit: number) => Promise<GraphPage>;
}

/** Everything the app needs from the machine; the Tauri backend, or a stand-in for dev and tests. */
export interface RepoBackend {
  /** Asks the user for a repository folder; `null` when cancelled. */
  pickFolder(): Promise<string | null>;
  open(path: string): Promise<{ info: RepoInfo; refs: RefInfo[] }>;
  close(repoId: string): Promise<void>;
  /** Graph of the repository without the commits only reachable from `hiddenRefs`. */
  graph(repoId: string, hiddenRefs: string[]): Promise<GraphSource>;
  /** Message and changed files of a commit. */
  commitDetail(repoId: string, commitId: string): Promise<CommitDetail>;
  /** Every ref of the repository (to refresh the sidebar). */
  refs(repoId: string): Promise<RefInfo[]>;
  /** Rows of these commits in the current graph (`null` when a commit is not in it). */
  locate(repoId: string, commitIds: string[]): Promise<Array<number | null>>;
  /**
   * Calls `handler` with the id of a repository whose refs moved (a commit, a fetch, a checkout
   * in a terminal...). Returns a function that stops listening.
   */
  onRepoChanged(handler: (repoId: string) => void): Promise<() => void>;
  /** The changes in the index and in the working tree. */
  status(repoId: string): Promise<WorkingStatus>;
  /** Diff of a changed file, for the staged or the unstaged side. */
  workingDiff(repoId: string, entry: StatusEntry, area: Area, force: boolean): Promise<FileDiff>;
  stage(repoId: string, paths: string[]): Promise<WorkingStatus>;
  unstage(repoId: string, paths: string[]): Promise<WorkingStatus>;
  stageHunk(repoId: string, path: string, hunkId: string): Promise<WorkingStatus>;
  unstageHunk(repoId: string, path: string, hunkId: string): Promise<WorkingStatus>;
  /** Throws away unstaged changes after saving a snapshot that brings them back. */
  discard(repoId: string, paths: string[]): Promise<DiscardResult>;
  snapshots(repoId: string): Promise<Snapshot[]>;
  restoreSnapshot(repoId: string, reference: string): Promise<DiscardResult>;
  commit(repoId: string, summary: string, body: string, amend: boolean): Promise<CommitResult>;
  undoCommit(repoId: string, before: string | null, after: string): Promise<WorkingStatus>;
  identity(repoId: string): Promise<Identity>;
  setIdentity(repoId: string, name: string, email: string, global: boolean): Promise<Identity>;
  setReadOnly(repoId: string, readOnly: boolean): Promise<void>;
  /** The git commands the app ran for a repository, oldest first. */
  activity(repoId: string): Promise<ActivityEntry[]>;
  clearActivity(repoId: string): Promise<void>;
  createBranch(
    repoId: string,
    name: string,
    start: string | null,
    checkout: boolean,
  ): Promise<WorkingStatus>;
  /** Switches branch (or detaches HEAD). Local changes in the way make it fail with their files. */
  checkout(
    repoId: string,
    target: CheckoutTarget,
    strategy: DirtyStrategy | null,
  ): Promise<CheckoutResult>;
  renameBranch(repoId: string, oldName: string, newName: string): Promise<WorkingStatus>;
  /** A branch with unmerged commits needs `force`. */
  deleteBranch(repoId: string, name: string, force: boolean): Promise<DeletedBranch>;
  restoreBranch(
    repoId: string,
    name: string,
    tip: string,
    upstream: string | null,
  ): Promise<WorkingStatus>;
  setUpstream(repoId: string, branch: string, upstream: string | null): Promise<WorkingStatus>;
  /** Annotated when a message is given. */
  createTag(
    repoId: string,
    name: string,
    target: string | null,
    message: string | null,
  ): Promise<WorkingStatus>;
  deleteTag(repoId: string, name: string): Promise<DeletedTag>;
  restoreTag(repoId: string, name: string, target: string): Promise<WorkingStatus>;
  stashes(repoId: string): Promise<StashEntry[]>;
  stashPush(
    repoId: string,
    message: string | null,
    includeUntracked: boolean,
    keepIndex: boolean,
  ): Promise<WorkingStatus>;
  stashApply(
    repoId: string,
    reference: string,
    id: string,
    restoreIndex: boolean,
  ): Promise<WorkingStatus>;
  /** Applies a stash and removes it (a conflict keeps it). */
  stashPop(
    repoId: string,
    reference: string,
    id: string,
    restoreIndex: boolean,
  ): Promise<WorkingStatus>;
  stashDrop(repoId: string, reference: string, id: string): Promise<DroppedStash>;
  stashRestore(repoId: string, id: string, message: string): Promise<WorkingStatus>;
  /** A conflict cancels the operation and changes nothing. */
  cherryPick(repoId: string, commit: string): Promise<CommitOpResult>;
  revert(repoId: string, commit: string): Promise<CommitOpResult>;
  undoReplay(repoId: string, before: string, after: string): Promise<WorkingStatus>;
  reset(repoId: string, target: string, mode: ResetMode): Promise<ResetResult>;
  undoReset(
    repoId: string,
    before: string,
    after: string,
    snapshot: string | null,
  ): Promise<DiscardResult>;
  /** The names of the remotes. */
  remotes(repoId: string): Promise<string[]>;
  /** Downloads new commits and refs (`remote` null: every remote). */
  fetch(repoId: string, opId: string, remote: string | null, prune: boolean): Promise<RemoteResult>;
  /** Fetches and integrates the upstream; `divergedBranches` means a strategy is needed. */
  pull(repoId: string, opId: string, strategy: PullStrategy | null): Promise<RemoteResult>;
  push(repoId: string, opId: string, options: PushOptions): Promise<RemoteResult>;
  deleteRemoteBranch(
    repoId: string,
    opId: string,
    remote: string,
    branch: string,
  ): Promise<RemoteResult>;
  /** Stops a running fetch, pull or push. */
  cancelOperation(opId: string): Promise<void>;
  /** Answers a question git asked (`null` refuses). */
  answerAskpass(opId: string, promptId: string, answer: string | null): Promise<void>;
  /** Calls `handler` with each line of progress of the running operations. */
  onRemoteProgress(handler: (event: RemoteProgressEvent) => void): Promise<() => void>;
  /** Calls `handler` when git or ssh asks for a secret or a confirmation. */
  onAskpass(handler: (event: AskpassEvent) => void): Promise<() => void>;
  /** Calls `handler` when a file of the working tree (or the index) changed. */
  onWorktreeChanged(handler: (repoId: string) => void): Promise<() => void>;
  /** Calls `handler` when a git command was added to the activity log. */
  onActivityChanged(handler: (repoId: string) => void): Promise<() => void>;
  /** Rows of the current graph matching `query` (see `searchCommits`). */
  search(repoId: string, query: string, limit: number): Promise<SearchResult>;
  /** Diff of one file of a commit; `force` allows files over the automatic size limit. */
  fileDiff(repoId: string, commitId: string, file: FileChange, force: boolean): Promise<FileDiff>;
}

export const isTauri = (): boolean => "__TAURI_INTERNALS__" in window;

export const tauriBackend: RepoBackend = {
  async pickFolder() {
    const selected = await open({ directory: true, multiple: false, title: "Open a repository" });
    return typeof selected === "string" ? selected : null;
  },
  async open(path) {
    const info = await openRepo(path);
    return { info, refs: await listRefs(info.id) };
  },
  close: closeRepo,
  async graph(repoId, hiddenRefs) {
    const meta = await getGraphMeta(repoId, hiddenRefs);
    return { meta, fetchPage: (cursor, limit) => getGraphPage(repoId, cursor, limit) };
  },
  status: getStatus,
  workingDiff: (repoId, entry, area, force) =>
    getWorkingDiff(repoId, entry.path, entry.oldPath, area, force),
  stage: stagePaths,
  unstage: unstagePaths,
  stageHunk,
  unstageHunk,
  discard: discardPaths,
  snapshots: listSnapshots,
  restoreSnapshot,
  commit: commitChanges,
  undoCommit,
  identity: getIdentity,
  setIdentity,
  setReadOnly,
  activity: getActivity,
  clearActivity,
  onWorktreeChanged: (handler) =>
    listen<{ repoId: string }>("worktree-changed", (event) => handler(event.payload.repoId)),
  onActivityChanged: (handler) =>
    listen<{ repoId: string }>("activity-changed", (event) => handler(event.payload.repoId)),
  createBranch,
  remotes: listRemotes,
  stashes: listStashes,
  stashPush,
  stashApply,
  stashPop,
  stashDrop,
  stashRestore,
  cherryPick,
  revert: revertCommit,
  undoReplay,
  reset: resetTo,
  undoReset,
  fetch: fetchRemote,
  pull: pullBranch,
  push: pushBranch,
  deleteRemoteBranch,
  cancelOperation,
  answerAskpass,
  onRemoteProgress: (handler) =>
    listen<RemoteProgressEvent>("remote-progress", (event) => handler(event.payload)),
  onAskpass: (handler) =>
    listen<AskpassEvent>("askpass-request", (event) => handler(event.payload)),
  checkout: checkoutTarget,
  renameBranch,
  deleteBranch,
  restoreBranch,
  setUpstream,
  createTag,
  deleteTag,
  restoreTag,
  commitDetail: getCommitDetail,
  search: searchCommits,
  refs: listRefs,
  locate: locateCommits,
  onRepoChanged: (handler) =>
    listen<{ repoId: string }>("repo-changed", (event) => handler(event.payload.repoId)),
  fileDiff: (repoId, commitId, file, force) =>
    getFileDiff(repoId, commitId, file.path, file.oldPath, force),
};
