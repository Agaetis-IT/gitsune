import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { HostedRepo } from "@/lib/remoteUrl";
import {
  type ActivityEntry,
  type StatusHint,
  getStatusHint,
  speedUpStatus,
  type RebasePlan,
  type RebaseResult,
  type TodoItem,
  rebaseOnto,
  rebasePreview,
  skipOperation,
  type ConflictFile,
  type MergeInfo,
  type MergeMode,
  type MergeResult,
  type Side,
  abortOperation,
  continueOperation,
  getConflictFile,
  mergeIntoCurrent,
  mergePreview,
  resolveConflictFile,
  resolveConflictSide,
  type CommitOpResult,
  type DroppedStash,
  type ReflogEntry,
  type ResetMode,
  type UndoResult,
  type ResetResult,
  type StashEntry,
  cherryPick,
  listReflog,
  listStashes,
  restoreHead,
  undoLast,
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
  listRemoteUrls,
  openExternal,
  openInEditor,
  openInTerminal,
  findPullRequest,
  listPullRequests,
  defaultBranch,
  createPullRequest,
  commitsBetween,
  listAuthors,
  autoFetch,
  listWorktrees,
  addWorktree,
  removeWorktree,
  lockWorktree,
  pruneWorktrees,
  type AuthorCount,
  type Blame,
  type AutoFetchResult,
  type NewPullRequest,
  type PullRequestList,
  type RangeCommit,
  type Worktree,
  type PullRequestLookup,
  type RemoteUrl,
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
  stageLines,
  stagePaths,
  undoCommit,
  unstageHunk,
  unstageLines,
  blameFile,
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
  /** Stages some lines of a hunk (indices into its lines). */
  stageLines(repoId: string, path: string, hunkId: string, lines: number[]): Promise<WorkingStatus>;
  unstageLines(
    repoId: string,
    path: string,
    hunkId: string,
    lines: number[],
  ): Promise<WorkingStatus>;
  /** Who wrote each line of a file, as of a commit (`null`: the working tree). */
  blame(repoId: string, commit: string | null, path: string): Promise<Blame>;
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
  /** The positions HEAD went through, newest first. */
  reflog(repoId: string): Promise<ReflogEntry[]>;
  /** Moves the branch back to a commit of the reflog; local changes in the way make it fail. */
  restoreHead(repoId: string, target: string): Promise<WorkingStatus>;
  /** Takes back the last operation that moved HEAD; doing it again is a redo. */
  undoLast(repoId: string): Promise<UndoResult>;
  undoReset(
    repoId: string,
    before: string,
    after: string,
    snapshot: string | null,
  ): Promise<DiscardResult>;
  mergeInfo(repoId: string, target: string): Promise<MergeInfo>;
  /** Conflicts are not an error: the outcome says so and the merge stays in progress. */
  merge(
    repoId: string,
    target: string,
    mode: MergeMode,
    message: string | null,
  ): Promise<MergeResult>;
  conflictFile(repoId: string, path: string): Promise<ConflictFile>;
  /** Writes the resolved content and marks the file resolved. */
  resolveFile(repoId: string, path: string, content: string): Promise<WorkingStatus>;
  /** Takes one side as it is (a side that deleted the file deletes it). */
  resolveSide(repoId: string, path: string, side: Side): Promise<WorkingStatus>;
  /** Finishes the operation in progress (commits the merge...). */
  continueOperation(repoId: string): Promise<WorkingStatus>;
  abortOperation(repoId: string): Promise<WorkingStatus>;
  rebasePlan(repoId: string, onto: string): Promise<RebasePlan>;
  /** Conflicts and a stop for `edit` are not errors: the rebase stays in progress. */
  rebase(
    repoId: string,
    onto: string,
    items: TodoItem[] | null,
    autostash: boolean,
  ): Promise<RebaseResult>;
  /** Leaves out the commit a rebase or a cherry-pick is stuck on. */
  skipOperation(repoId: string): Promise<WorkingStatus>;
  /** How long `git status` takes and whether the untracked cache would help. */
  statusHint(repoId: string): Promise<StatusHint>;
  /** Switches on git's untracked cache for this repository. */
  speedUpStatus(repoId: string): Promise<StatusHint>;
  /** The names of the remotes. */
  remotes(repoId: string): Promise<string[]>;
  /** The remotes with their fetch addresses. */
  remoteUrls(repoId: string): Promise<RemoteUrl[]>;
  /** Opens an `https` address in the browser. */
  openExternal(url: string): Promise<void>;
  /** Opens the repository (`path` null) or a file of it in an editor (`zed`, `vscode`...). */
  openInEditor(repoId: string, editor: string, path: string | null): Promise<void>;
  /** Opens a terminal in the repository. */
  openInTerminal(repoId: string, terminal: string): Promise<void>;
  /** The open pull request of a branch (needs the token of the host). */
  findPullRequest(repo: HostedRepo, branch: string): Promise<PullRequestLookup>;
  /** Every open pull request of the repository, with review and build state. */
  listPullRequests(repo: HostedRepo): Promise<PullRequestList>;
  /** The default branch of the repository on its host; `null` when not signed in. */
  defaultBranch(repo: HostedRepo): Promise<string | null>;
  /** Creates a pull request and returns its number. */
  createPullRequest(repo: HostedRepo, request: NewPullRequest): Promise<number>;
  /** The commits `head` has beyond `base`, oldest first. */
  commitsBetween(repoId: string, base: string, head: string): Promise<RangeCommit[]>;
  /** The authors of the repository, the most active first. */
  authors(repoId: string): Promise<AuthorCount[]>;
  /** Fetches every remote in the background, never asking a question; fails quietly. */
  autoFetch(repoId: string): Promise<AutoFetchResult>;
  /** The working trees of the repository, the main one first. */
  worktrees(repoId: string): Promise<Worktree[]>;
  /** Checks a branch out in a new folder (created from `start` when `newBranch`). */
  addWorktree(
    repoId: string,
    path: string,
    branch: string,
    newBranch: boolean,
    start: string | null,
  ): Promise<void>;
  /** Removes a linked working tree; one with changes is refused unless `force`. */
  removeWorktree(repoId: string, path: string, force: boolean): Promise<void>;
  lockWorktree(repoId: string, path: string, lock: boolean): Promise<void>;
  /** Forgets the working trees whose folder is gone. */
  pruneWorktrees(repoId: string): Promise<void>;
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
  stageLines,
  unstageHunk,
  unstageLines,
  blame: blameFile,
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
  remoteUrls: listRemoteUrls,
  openExternal,
  openInEditor,
  openInTerminal,
  findPullRequest,
  listPullRequests,
  defaultBranch,
  createPullRequest,
  commitsBetween,
  authors: listAuthors,
  autoFetch,
  worktrees: listWorktrees,
  addWorktree,
  removeWorktree,
  lockWorktree,
  pruneWorktrees,
  statusHint: getStatusHint,
  speedUpStatus,
  rebasePlan: rebasePreview,
  rebase: rebaseOnto,
  skipOperation,
  mergeInfo: mergePreview,
  merge: mergeIntoCurrent,
  conflictFile: getConflictFile,
  resolveFile: resolveConflictFile,
  resolveSide: resolveConflictSide,
  continueOperation,
  abortOperation,
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
  reflog: listReflog,
  restoreHead,
  undoLast,
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
