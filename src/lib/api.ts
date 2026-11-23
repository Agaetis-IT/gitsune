import { invoke } from "@tauri-apps/api/core";

// Types mirror the Rust structs serialized by the backend (camelCase).

export interface RepoInfo {
  id: string;
  path: string;
  name: string;
  /** Checked-out branch, `null` when HEAD is detached or unborn. */
  headBranch: string | null;
  isBare: boolean;
}

export type RefKind = "local" | "remote" | "tag";

export interface RefInfo {
  name: string;
  fullName: string;
  kind: RefKind;
  /** Commit id the ref points at (annotated tags are peeled). */
  target: string;
  isHead: boolean;
}

export type LaneKind = "branch" | "merged" | "anonymous";

/** One branch line: a vertical segment at `col` from `firstRow` to `lastRow`. */
export interface Lane {
  id: number;
  col: number;
  firstRow: number;
  lastRow: number;
  /** Begins as a connector leaving a merge commit (otherwise at its own tip, in `firstRow`). */
  forked: boolean;
  /** Ends by bending into another lane (otherwise at its own last commit, in `lastRow`). */
  joins: boolean;
  name: string | null;
  kind: LaneKind;
  /** Palette index, `null` for anonymous lanes (neutral color). */
  color: number | null;
}

/** Where the column change of a connector happens within its span. */
export type Bend = "start" | "end";

/** A non-vertical connector between a row and the next one. */
export interface Edge {
  lane: number;
  fromCol: number;
  toCol: number;
  bend: Bend;
}

/** A ref that seeds the graph, with the row of the commit it points at. */
export interface GraphRef extends RefInfo {
  row: number;
  /** Lane of that commit, to highlight the branch. */
  lane: number;
}

export interface GraphMeta {
  /** Number of rows (commits). */
  total: number;
  maxCols: number;
  lanes: Lane[];
  /** Visible refs only (hidden refs are left out). */
  refs: GraphRef[];
  loadMs: number;
  layoutMs: number;
}

export interface GraphRow {
  row: number;
  id: string;
  parents: string[];
  summary: string;
  author: string;
  /** Author time, seconds since the Unix epoch. */
  time: number;
  col: number;
  lane: number;
  edges: Edge[];
  refs: RefInfo[];
}

export interface GraphPage {
  start: number;
  rows: GraphRow[];
  /** Cursor of the next page, `null` at the end of the graph. */
  nextCursor: number | null;
  total: number;
}

export type ChangeStatus = "added" | "modified" | "deleted" | "renamed" | "copied" | "typeChanged";

/** A file changed by a commit (against its first parent). */
export interface FileChange {
  path: string;
  /** Previous path of a renamed or copied file. */
  oldPath: string | null;
  status: ChangeStatus;
  /** Added and removed lines; `null` when not counted (binary, large, or too many files). */
  additions: number | null;
  deletions: number | null;
  binary: boolean;
}

export interface CommitDetail {
  id: string;
  parents: string[];
  authorName: string;
  authorEmail: string;
  /** Seconds since the Unix epoch. */
  authorTime: number;
  /** Full message. */
  message: string;
  files: FileChange[];
  /** The commit changes more files than `files` lists. */
  filesTruncated: boolean;
}

export type DiffLineKind = "context" | "add" | "delete";

export interface DiffLine {
  kind: DiffLineKind;
  oldNo: number | null;
  newNo: number | null;
  /** The line without its terminator. */
  text: string;
  /** The file ends here without a final newline. */
  noEol: boolean;
}

export interface DiffHunk {
  /** Identifies this hunk (position and exact content), to stage or unstage it. */
  id: string;
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  /** `@@ -a,b +c,d @@` */
  header: string;
  lines: DiffLine[];
}

export interface FileDiff {
  path: string;
  oldPath: string | null;
  status: ChangeStatus;
  binary: boolean;
  /** Only the commit id of a submodule changed. */
  submodule: boolean;
  /** A side is bigger than the limit: no diff was computed (see `canForce`). */
  tooLarge: boolean;
  canForce: boolean;
  /** The diff was cut at the line limit. */
  truncated: boolean;
  oldSize: number | null;
  newSize: number | null;
  additions: number;
  deletions: number;
  hunks: DiffHunk[];
}

/** Largest page the backend returns. */
export const MAX_PAGE_SIZE = 1000;

export const openRepo = (path: string) => invoke<RepoInfo>("open_repo", { path });
export const closeRepo = (repoId: string) => invoke<void>("close_repo", { repoId });
export const listRefs = (repoId: string) => invoke<RefInfo[]>("list_refs", { repoId });

/**
 * Reads and lays out the graph; call it before requesting pages. Commits only reachable
 * from `hiddenRefs` (full ref names such as `refs/heads/main`) are left out.
 */
export const getGraphMeta = (repoId: string, hiddenRefs: string[] = []) =>
  invoke<GraphMeta>("get_graph_meta", { repoId, hiddenRefs });

export const getGraphPage = (repoId: string, cursor: number, limit: number) =>
  invoke<GraphPage>("get_graph_page", { repoId, cursor, limit });

export const getCommitDetail = (repoId: string, id: string) =>
  invoke<CommitDetail>("get_commit_detail", { repoId, id });

/** Diff of a file of a commit; files over 1 MiB are only diffed with `force`. */
export const getFileDiff = (
  repoId: string,
  id: string,
  path: string,
  oldPath: string | null,
  force: boolean,
) => invoke<FileDiff>("get_file_diff", { repoId, id, path, oldPath, force });

export interface SearchResult {
  /** Matching rows, newest first, at most the requested limit. */
  rows: number[];
  /** Number of matching commits (can exceed `rows.length`). */
  total: number;
}

/** Most rows a search returns. */
export const MAX_SEARCH_RESULTS = 2000;

/**
 * Commits whose message or author contains every word of `query` (case-insensitive), or
 * whose id starts with it. Searches the commits of the current graph.
 */
export const searchCommits = (repoId: string, query: string, limit: number) =>
  invoke<SearchResult>("search_commits", { repoId, query, limit });

/**
 * Rows of the given commit ids in the current graph (`null` for commits it does not contain).
 * Used after a refresh to find where the selected commit and the top of the view went.
 */
export const locateCommits = (repoId: string, ids: string[]) =>
  invoke<Array<number | null>>("locate_commits", { repoId, ids });

// ---------- working tree: status, staging, commits ----------

export type FileState =
  "added" | "modified" | "deleted" | "renamed" | "copied" | "typeChanged" | "untracked";

export interface StatusEntry {
  path: string;
  oldPath: string | null;
  state: FileState;
  submodule: boolean;
}

export interface ConflictEntry {
  path: string;
  /** The two-letter code of git (`UU`, `AA`, `DU`...). */
  code: string;
}

export interface WorkingStatus {
  branch: string | null;
  detached: boolean;
  /** Commit HEAD points at, `null` in a repository without commits. */
  head: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  /** Changes in the index: what the next commit will contain. */
  staged: StatusEntry[];
  /** Changes in the working tree not in the index, untracked files included. */
  unstaged: StatusEntry[];
  conflicted: ConflictEntry[];
}

/** Which side of the index a working tree diff is about. */
export type Area = "staged" | "unstaged";

export type GitErrorKind =
  | "readOnly"
  | "locked"
  | "needsIdentity"
  | "hookFailed"
  | "signingFailed"
  | "nothingToCommit"
  | "staleHunk"
  | "invalidName"
  | "alreadyExists"
  | "wouldOverwriteChanges"
  | "notFullyMerged"
  | "branchCheckedOut"
  | "other";

/** A git operation that failed, with what the interface needs to react to it. */
export class GitCommandError extends Error {
  constructor(
    readonly kind: GitErrorKind,
    message: string,
    /** Files the error is about (those a checkout would overwrite). */
    readonly files: string[] = [],
  ) {
    super(message);
    this.name = "GitCommandError";
  }
}

/** Converts what a command rejects with (`{ kind, message }` or a string) into an error. */
export function toGitError(error: unknown): GitCommandError {
  if (error instanceof GitCommandError) return error;
  if (typeof error === "object" && error !== null && "message" in error) {
    const { kind, message, files } = error as {
      kind?: GitErrorKind;
      message: string;
      files?: string[];
    };
    return new GitCommandError(kind ?? "other", String(message), Array.isArray(files) ? files : []);
  }
  return new GitCommandError("other", String(error));
}

async function gitInvoke<T>(command: string, args: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw toGitError(error);
  }
}

export interface CommitResult {
  id: string;
  /** What HEAD was before (`null` for the first commit), to undo the commit. */
  before: string | null;
  status: WorkingStatus;
}

/** The content of some files saved before they were discarded or overwritten. */
export interface Snapshot {
  reference: string;
  id: string;
  /** Seconds since the Unix epoch. */
  time: number;
  description: string;
  paths: string[];
}

export interface DiscardResult {
  snapshot: Snapshot | null;
  status: WorkingStatus;
}

export interface Identity {
  name: string | null;
  email: string | null;
}

export interface ActivityEntry {
  id: number;
  /** Milliseconds since the Unix epoch. */
  startedAt: number;
  command: string;
  durationMs: number;
  exitCode: number;
  ok: boolean;
  stdout: string;
  stderr: string;
  truncated: boolean;
}

export const getStatus = (repoId: string) => gitInvoke<WorkingStatus>("get_status", { repoId });

export const getWorkingDiff = (
  repoId: string,
  path: string,
  oldPath: string | null,
  area: Area,
  force: boolean,
) => gitInvoke<FileDiff>("get_working_diff", { repoId, path, oldPath, area, force });

export const stagePaths = (repoId: string, paths: string[]) =>
  gitInvoke<WorkingStatus>("stage_paths", { repoId, paths });
export const unstagePaths = (repoId: string, paths: string[]) =>
  gitInvoke<WorkingStatus>("unstage_paths", { repoId, paths });
export const stageHunk = (repoId: string, path: string, hunkId: string) =>
  gitInvoke<WorkingStatus>("stage_hunk", { repoId, path, hunkId });
export const unstageHunk = (repoId: string, path: string, hunkId: string) =>
  gitInvoke<WorkingStatus>("unstage_hunk", { repoId, path, hunkId });

/** Throws away unstaged changes after saving a snapshot that brings them back. */
export const discardPaths = (repoId: string, paths: string[]) =>
  gitInvoke<DiscardResult>("discard_paths", { repoId, paths });
export const listSnapshots = (repoId: string) =>
  gitInvoke<Snapshot[]>("list_snapshots", { repoId });
export const restoreSnapshot = (repoId: string, reference: string) =>
  gitInvoke<DiscardResult>("restore_snapshot", { repoId, reference });

export const commitChanges = (repoId: string, summary: string, body: string, amend: boolean) =>
  gitInvoke<CommitResult>("commit_changes", { repoId, summary, body, amend });
export const undoCommit = (repoId: string, before: string | null, after: string) =>
  gitInvoke<WorkingStatus>("undo_commit", { repoId, before, after });

export const getIdentity = (repoId: string) => gitInvoke<Identity>("get_identity", { repoId });
export const setIdentity = (repoId: string, name: string, email: string, global: boolean) =>
  gitInvoke<Identity>("set_identity", { repoId, name, email, global });

export const setReadOnly = (repoId: string, readOnly: boolean) =>
  gitInvoke<void>("set_read_only", { repoId, readOnly });
export const getActivity = (repoId: string) =>
  gitInvoke<ActivityEntry[]>("get_activity", { repoId });
export const clearActivity = (repoId: string) => gitInvoke<void>("clear_activity", { repoId });

// ---------- branches, checkout and tags ----------

/** Where a checkout goes. */
export type CheckoutTarget =
  | { kind: "branch"; name: string }
  /** A remote-tracking branch (`origin/x`): the local branch is created if needed. */
  | { kind: "remote"; name: string }
  /** A commit, as a detached HEAD. */
  | { kind: "commit"; id: string }
  | { kind: "tag"; name: string };

/** What to do with local changes that a checkout would overwrite. */
export type DirtyStrategy = "stash" | "discard";

export interface CheckoutResult {
  status: WorkingStatus;
  /** Made before discarding changes; restoring it brings them back. */
  snapshot: Snapshot | null;
  /** The changes were stashed and applied again after the switch. */
  stashed: boolean;
  /** Applying the stash again conflicted: the changes are still in the stash. */
  stashConflict: boolean;
}

export interface DeletedBranch {
  name: string;
  /** The commit the branch pointed at. */
  tip: string;
  upstream: string | null;
  status: WorkingStatus;
}

export interface DeletedTag {
  name: string;
  /** What the tag pointed at: the commit, or the tag object of an annotated tag. */
  target: string;
  status: WorkingStatus;
}

export const createBranch = (
  repoId: string,
  name: string,
  start: string | null,
  checkout: boolean,
) => gitInvoke<WorkingStatus>("create_branch", { repoId, name, start, checkout });

export const checkoutTarget = (
  repoId: string,
  target: CheckoutTarget,
  strategy: DirtyStrategy | null,
) => gitInvoke<CheckoutResult>("checkout_target", { repoId, target, strategy });

export const renameBranch = (repoId: string, oldName: string, newName: string) =>
  gitInvoke<WorkingStatus>("rename_branch", { repoId, oldName, newName });

/** A branch with unmerged commits needs `force` (the error is `notFullyMerged` without it). */
export const deleteBranch = (repoId: string, name: string, force: boolean) =>
  gitInvoke<DeletedBranch>("delete_branch", { repoId, name, force });

export const restoreBranch = (repoId: string, name: string, tip: string, upstream: string | null) =>
  gitInvoke<WorkingStatus>("restore_branch", { repoId, name, tip, upstream });

export const setUpstream = (repoId: string, branch: string, upstream: string | null) =>
  gitInvoke<WorkingStatus>("set_upstream", { repoId, branch, upstream });

/** Creates a tag at `target` (HEAD by default); annotated when a message is given. */
export const createTag = (
  repoId: string,
  name: string,
  target: string | null,
  message: string | null,
) => gitInvoke<WorkingStatus>("create_tag", { repoId, name, target, message });

export const deleteTag = (repoId: string, name: string) =>
  gitInvoke<DeletedTag>("delete_tag", { repoId, name });

export const restoreTag = (repoId: string, name: string, target: string) =>
  gitInvoke<WorkingStatus>("restore_tag", { repoId, name, target });
