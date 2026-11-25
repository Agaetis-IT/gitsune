import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  type ActivityEntry,
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
  commitDetail: getCommitDetail,
  search: searchCommits,
  refs: listRefs,
  locate: locateCommits,
  onRepoChanged: (handler) =>
    listen<{ repoId: string }>("repo-changed", (event) => handler(event.payload.repoId)),
  fileDiff: (repoId, commitId, file, force) =>
    getFileDiff(repoId, commitId, file.path, file.oldPath, force),
};
