import { invoke } from "@tauri-apps/api/core";
import type { HostedRepo } from "@/lib/remoteUrl";

// Types mirror the Rust structs serialized by the backend (camelCase).

export interface RepoInfo {
  id: string;
  path: string;
  name: string;
  /** Checked-out branch, `null` when HEAD is detached or unborn. */
  headBranch: string | null;
  isBare: boolean;
  /** A linked working tree (made by `git worktree add`), not the main one. */
  isLinked: boolean;
}

/**
 * The name that, among the hidden refs of a repository, leaves every stash out of the graph
 * (the same as `STASH_REF` of the backend).
 */
export const STASH_REF = "refs/stash";

/** A stash is a label of the graph only: it is never in the lists of branches and tags. */
export type RefKind = "local" | "remote" | "tag" | "stash";

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

/** A remote with the address it is fetched from. */
export interface RemoteUrl {
  name: string;
  url: string;
  /** The real host of the address, with an SSH alias replaced by its HostName. */
  host?: string | null;
}

/** What the lookup of the pull request of a branch found. */
export type PullRequestLookup =
  { state: "signedOut" } | { state: "none" } | { state: "found"; id: number; title: string };

export type PrReview = "none" | "approved" | "changesRequested" | "pending";
export type PrChecks = "none" | "pending" | "success" | "failure";

/** An open pull request, by its source branch, with the state of its review and builds. */
export interface BranchPr {
  branch: string;
  id: number;
  title: string;
  draft: boolean;
  review: PrReview;
  checks: PrChecks;
}

export type PullRequestList = { state: "signedOut" } | { state: "listed"; items: BranchPr[] };

/** What the user filled in to create a pull request. */
export interface NewPullRequest {
  source: string;
  target: string;
  title: string;
  description: string;
  draft: boolean;
}

/** A working tree of the repository (`git worktree list`). */
export interface Worktree {
  path: string;
  /** The commit checked out there. */
  head: string;
  /** `null` for a detached HEAD or a bare repository. */
  branch: string | null;
  detached: boolean;
  bare: boolean;
  /** The first one listed is the main working tree. */
  isMain: boolean;
  /** Set (maybe empty) when locked: its reason. */
  locked: string | null;
  /** Set when git would prune it: its folder is gone. */
  prunable: string | null;
  /** Uncommitted changes there; `null` when it could not be told. */
  dirty: boolean | null;
}

/** A commit that wrote lines of a file. */
export interface BlameCommit {
  id: string;
  author: string;
  /** Author time, seconds since the Unix epoch. */
  time: number;
  summary: string;
  /** The lines are in the working tree only. */
  uncommitted: boolean;
}

export interface Blame {
  commits: BlameCommit[];
  /** One entry per line of the file; `commit` indexes `commits`. */
  lines: Array<{ commit: number; text: string }>;
  /** The file has more lines than are returned. */
  truncated: boolean;
}

/** What the system the application runs on offers. */
export interface PlatformInfo {
  os: "macos" | "linux" | "windows";
  editors: Array<{ id: string; label: string }>;
  terminals: Array<{ id: string; label: string }>;
  /** "Finder", "Explorer" or "the file manager". */
  fileManager: string;
  /** There is a native menu bar (the other systems use the command palette). */
  nativeMenu: boolean;
}

/** What the application can tell about its surroundings, with nothing personal in it. */
export interface Diagnostics {
  appVersion: string;
  os: string;
  arch: string;
  pathDirs: string[];
  tools: Array<{ name: string; found: boolean; path: string | null; version: string | null }>;
  hosts: Array<{ host: string; tool: string; state: "signedIn" | "signedOut" | "missing" }>;
  sshAgent: string;
  /** Keys the agent holds; `null` when it cannot be asked. */
  sshKeys: number | null;
  signing: { format: string | null; enabled: boolean; hasKey: boolean };
}

/** A branch that received commits from an automatic fetch. */
export interface BranchUpdate {
  branch: string;
  upstream: string;
  newCommits: number;
}

export interface AutoFetchResult {
  /** Commits waiting to be pulled, over every branch that has an upstream. */
  behindTotal: number;
  updates: BranchUpdate[];
}

/** An author of the repository and how many commits they wrote. */
export interface AuthorCount {
  name: string;
  count: number;
}

/** A commit of a branch that its target does not have, for the text of a pull request. */
export interface RangeCommit {
  id: string;
  summary: string;
}

const plainError = (error: unknown): Error =>
  new Error(typeof error === "string" ? error : String(error));

/** Commands that fail with a plain message (not a git error). */
async function plainInvoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw plainError(error);
  }
}

export const listRemoteUrls = (repoId: string) =>
  gitInvoke<RemoteUrl[]>("list_remote_urls", { repoId });
export const openExternal = (url: string) => plainInvoke<void>("open_external", { url });
export const openInEditor = (repoId: string, editor: string, path: string | null) =>
  gitInvoke<void>("open_in_editor", { repoId, editor, path });
export const openInTerminal = (repoId: string, terminal: string) =>
  gitInvoke<void>("open_in_terminal", { repoId, terminal });
/** Whether the user is signed in with the command line tool of a host (`az`, `gh`). */
export interface HostStatus {
  host: string;
  /** The tool that holds the sign-in: "Azure CLI", "GitHub CLI". */
  tool: string;
  state: "signedIn" | "signedOut" | "missing";
  account: string | null;
}

export const hostStatuses = () => plainInvoke<HostStatus[]>("host_statuses");
/** Opens the sign-in page of the Azure CLI (`az login`) and resolves when the user is done. */
export const azureSignIn = () => plainInvoke<void>("azure_sign_in");
export const findPullRequest = (repo: HostedRepo, branch: string) =>
  plainInvoke<PullRequestLookup>("find_pull_request", { repo, branch });
export const listPullRequests = (repo: HostedRepo) =>
  plainInvoke<PullRequestList>("list_pull_requests", { repo });
/** The default branch of the repository on its host; `null` when not signed in. */
export const defaultBranch = (repo: HostedRepo) =>
  plainInvoke<string | null>("default_branch", { repo });
/** Creates the pull request and returns its number. */
export const createPullRequest = (repo: HostedRepo, request: NewPullRequest) =>
  plainInvoke<number>("create_pull_request", { repo, request });
/** Fetches every remote without ever asking the user anything. */
export const autoFetch = (repoId: string) => gitInvoke<AutoFetchResult>("auto_fetch", { repoId });
export const listWorktrees = (repoId: string) =>
  gitInvoke<Worktree[]>("list_worktrees", { repoId });
export const addWorktree = (
  repoId: string,
  path: string,
  branch: string,
  newBranch: boolean,
  start: string | null,
) => gitInvoke<void>("add_worktree", { repoId, path, branch, newBranch, start });
export const removeWorktree = (repoId: string, path: string, force: boolean) =>
  gitInvoke<void>("remove_worktree", { repoId, path, force });
export const lockWorktree = (repoId: string, path: string, lock: boolean) =>
  gitInvoke<void>("lock_worktree", { repoId, path, lock });
export const pruneWorktrees = (repoId: string) => gitInvoke<void>("prune_worktrees", { repoId });
export const blameFile = (repoId: string, commit: string | null, path: string) =>
  gitInvoke<Blame>("blame_file", { repoId, commit, path });
export const getPlatformInfo = () => plainInvoke<PlatformInfo>("platform_info");
/** The latest published release of the application, compared with the running version. */
export interface UpdateCheck {
  current: string;
  latest: string;
  available: boolean;
  /** The page of the release, where the packages are. */
  url: string;
  /** What is new, as written in the release (plain text, possibly shortened). */
  notes: string;
}

export const checkForUpdates = () => plainInvoke<UpdateCheck>("check_for_updates");
export const getDiagnostics = () => plainInvoke<Diagnostics>("get_diagnostics");
export const listAuthors = (repoId: string) => gitInvoke<AuthorCount[]>("list_authors", { repoId });
export const commitsBetween = (repoId: string, base: string, head: string) =>
  gitInvoke<RangeCommit[]>("commits_between", { repoId, base, head });

export const revealInFinder = (path: string) => invoke<void>("reveal_in_finder", { path });
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

export type OperationKind = "merge" | "rebase" | "cherryPick" | "revert";

/** A merge, cherry-pick, revert or rebase that is waiting to be finished or cancelled. */
export interface Operation {
  kind: OperationKind;
  /** The first line of the message git prepared (`Merge branch 'x'`), when there is one. */
  message: string;
}

export interface WorkingStatus {
  /** `null` when nothing is waiting. */
  operation: Operation | null;
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
  | "authFailed"
  | "unreachable"
  | "pushRejected"
  | "noUpstream"
  | "divergedBranches"
  | "conflict"
  | "cancelled"
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
export const stageLines = (repoId: string, path: string, hunkId: string, lines: number[]) =>
  gitInvoke<WorkingStatus>("stage_lines", { repoId, path, hunkId, lines });
export const unstageLines = (repoId: string, path: string, hunkId: string, lines: number[]) =>
  gitInvoke<WorkingStatus>("unstage_lines", { repoId, path, hunkId, lines });
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
export const deleteBranch = (
  repoId: string,
  name: string,
  force: boolean,
  removeTracking: boolean,
) => gitInvoke<DeletedBranch>("delete_branch", { repoId, name, force, removeTracking });

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

// ---------- fetch, pull and push ----------

/** How a pull reconciles the remote branch with the local one. */
export type PullStrategy = "fastForwardOnly" | "merge" | "rebase";

/** What to push and how. */
export interface PushOptions {
  /** The remote to push to; the upstream of the current branch when `null`. */
  remote: string | null;
  /** The local branch to push; the current one when `null`. */
  branch: string | null;
  /** Make the remote branch the upstream of the local one. */
  setUpstream: boolean;
  /** Overwrite the remote branch unless someone pushed since the last fetch. */
  forceWithLease: boolean;
}

export interface RemoteResult {
  status: WorkingStatus;
  /** What git said once done (the refs updated, `Already up to date.`...). */
  summary: string;
}

/** A line of progress of a running fetch, pull or push. */
export interface RemoteProgressEvent {
  repoId: string;
  opId: string;
  line: string;
}

/** git or ssh needs a secret or a confirmation to go on. */
export interface AskpassEvent {
  repoId: string;
  opId: string;
  promptId: string;
  prompt: string;
}

/** `opId` names the operation so it can be cancelled and its progress followed. */
export const fetchRemote = (repoId: string, opId: string, remote: string | null, prune: boolean) =>
  gitInvoke<RemoteResult>("fetch_remote", { repoId, opId, remote, prune });

export const pullBranch = (repoId: string, opId: string, strategy: PullStrategy | null) =>
  gitInvoke<RemoteResult>("pull_branch", { repoId, opId, strategy });

export const pushBranch = (repoId: string, opId: string, options: PushOptions) =>
  gitInvoke<RemoteResult>("push_branch", { repoId, opId, options });

export const deleteRemoteBranch = (repoId: string, opId: string, remote: string, branch: string) =>
  gitInvoke<RemoteResult>("delete_remote_branch", { repoId, opId, remote, branch });

export const listRemotes = (repoId: string) => gitInvoke<string[]>("list_remotes", { repoId });

// The commands below give the remotes as they are afterwards.
export const addRemote = (repoId: string, name: string, url: string) =>
  gitInvoke<RemoteUrl[]>("add_remote", { repoId, name, url });
export const renameRemote = (repoId: string, name: string, newName: string) =>
  gitInvoke<RemoteUrl[]>("rename_remote", { repoId, name, newName });
export const setRemoteUrl = (repoId: string, name: string, url: string) =>
  gitInvoke<RemoteUrl[]>("set_remote_url", { repoId, name, url });
export const removeRemote = (repoId: string, name: string) =>
  gitInvoke<RemoteUrl[]>("remove_remote", { repoId, name });

/** Pushes one tag to a remote, or every tag when `tag` is `null`. */
export const pushTags = (repoId: string, opId: string, remote: string, tag: string | null) =>
  gitInvoke<RemoteResult>("push_tags", { repoId, opId, remote, tag });

/**
 * Clones `url` into the folder `name` of `parent` and gives the path of the new repository.
 * `ownerId` is the tab it was started from: the progress events carry it.
 */
export const cloneRepository = (
  ownerId: string,
  opId: string,
  url: string,
  parent: string,
  name: string,
) => gitInvoke<string>("clone_repository", { ownerId, opId, url, parent, name });

export const cancelOperation = (opId: string) => gitInvoke<void>("cancel_operation", { opId });

/** Answers a question git asked; `null` refuses (the operation then fails). */
export const answerAskpass = (opId: string, promptId: string, answer: string | null) =>
  gitInvoke<void>("answer_askpass", { opId, promptId, answer });

// ---------- stash, cherry-pick, revert and reset ----------

export interface StashEntry {
  /** Position in the list (0 is the most recent); it shifts when a stash is dropped. */
  index: number;
  /** `stash@{n}`. */
  reference: string;
  id: string;
  message: string;
  /** Seconds since the Unix epoch. */
  time: number;
}

/** A stash that was dropped, with what brings it back. */
export interface DroppedStash {
  id: string;
  message: string;
  status: WorkingStatus;
}

/** HEAD before and after a cherry-pick or a revert: what Undo needs. */
export interface CommitOpResult {
  status: WorkingStatus;
  before: string;
  after: string;
}

/** `soft` keeps the changes staged, `mixed` unstages them, `hard` discards them. */
export type ResetMode = "soft" | "mixed" | "hard";

export interface ResetResult {
  status: WorkingStatus;
  before: string;
  after: string;
  /** Made before a hard reset discarded uncommitted changes. */
  snapshot: Snapshot | null;
}

export const listStashes = (repoId: string) => gitInvoke<StashEntry[]>("list_stashes", { repoId });

export const stashPush = (
  repoId: string,
  message: string | null,
  includeUntracked: boolean,
  keepIndex: boolean,
) => gitInvoke<WorkingStatus>("stash_push", { repoId, message, includeUntracked, keepIndex });

/** `id` is the stash commit seen at `reference`: a list that changed meanwhile is refused. */
export const stashApply = (repoId: string, reference: string, id: string, restoreIndex: boolean) =>
  gitInvoke<WorkingStatus>("stash_apply", { repoId, reference, id, restoreIndex });

export const stashPop = (repoId: string, reference: string, id: string, restoreIndex: boolean) =>
  gitInvoke<WorkingStatus>("stash_pop", { repoId, reference, id, restoreIndex });

export const stashDrop = (repoId: string, reference: string, id: string) =>
  gitInvoke<DroppedStash>("stash_drop", { repoId, reference, id });

export const stashRestore = (repoId: string, id: string, message: string) =>
  gitInvoke<WorkingStatus>("stash_restore", { repoId, id, message });

export const cherryPick = (repoId: string, commit: string) =>
  gitInvoke<CommitOpResult>("cherry_pick", { repoId, commit });

export const revertCommit = (repoId: string, commit: string) =>
  gitInvoke<CommitOpResult>("revert_commit", { repoId, commit });

export const undoReplay = (repoId: string, before: string, after: string) =>
  gitInvoke<WorkingStatus>("undo_replay", { repoId, before, after });

export const resetTo = (repoId: string, target: string, mode: ResetMode) =>
  gitInvoke<ResetResult>("reset_to", { repoId, target, mode });

export const undoReset = (repoId: string, before: string, after: string, snapshot: string | null) =>
  gitInvoke<DiscardResult>("undo_reset", { repoId, before, after, snapshot });

/** A reset of any local branch. `before` is `null` when the call created the branch. */
export interface BranchResetResult {
  status: WorkingStatus;
  branch: string;
  before: string | null;
  after: string;
  /** The checked-out branch: the reset went through HEAD and `undoReset` takes it back. */
  current: boolean;
  snapshot: Snapshot | null;
}

export const resetBranch = (repoId: string, branch: string, target: string, mode: ResetMode) =>
  gitInvoke<BranchResetResult>("reset_branch", { repoId, branch, target, mode });

export const undoBranchReset = (
  repoId: string,
  branch: string,
  before: string | null,
  after: string,
) => gitInvoke<WorkingStatus>("undo_branch_reset", { repoId, branch, before, after });

export interface ReflogEntry {
  /** The commit HEAD pointed at after the operation. */
  id: string;
  /** What git recorded, e.g. `commit: Add a file`. */
  message: string;
  /** Seconds since the Unix epoch. */
  time: number;
  /** HEAD points at this commit now. */
  current: boolean;
}

export interface UndoResult {
  status: WorkingStatus;
  /** What was undone, e.g. "the commit". */
  undone: string;
}

export const listReflog = (repoId: string) => gitInvoke<ReflogEntry[]>("list_reflog", { repoId });

export const restoreHead = (repoId: string, target: string) =>
  gitInvoke<WorkingStatus>("restore_head", { repoId, target });

export const undoLast = (repoId: string) => gitInvoke<UndoResult>("undo_last", { repoId });

// ---------- merge and conflicts ----------

export type MergeMode = "default" | "noFastForward" | "fastForwardOnly" | "squash";

export interface MergeInfo {
  /** The target is already part of the current branch. */
  upToDate: boolean;
  canFastForward: boolean;
  /** Commits of the target that the current branch does not have. */
  commits: number;
}

export type MergeOutcome = "upToDate" | "fastForward" | "merged" | "squashed" | "conflicts";

export interface MergeResult {
  status: WorkingStatus;
  outcome: MergeOutcome;
  before: string;
  after: string;
}

/** The three versions of a conflicted file and what the working tree holds now. */
export interface ConflictFile {
  path: string;
  /** The two-letter code of git (`UU` both modified, `UD` deleted by them...). */
  code: string;
  ours: string | null;
  theirs: string | null;
  base: string | null;
  /** The file in the working tree: with conflict markers, or already edited. */
  worktree: string | null;
  /** Binary or too large to edit here: only a side can be chosen. */
  chooseSideOnly: boolean;
}

export type Side = "ours" | "theirs";

export const mergePreview = (repoId: string, target: string) =>
  gitInvoke<MergeInfo>("merge_preview", { repoId, target });

export const mergeIntoCurrent = (
  repoId: string,
  target: string,
  mode: MergeMode,
  message: string | null,
) => gitInvoke<MergeResult>("merge_into_current", { repoId, target, mode, message });

export const getConflictFile = (repoId: string, path: string) =>
  gitInvoke<ConflictFile>("get_conflict_file", { repoId, path });

export const resolveConflictFile = (repoId: string, path: string, content: string) =>
  gitInvoke<WorkingStatus>("resolve_conflict_file", { repoId, path, content });

export const resolveConflictSide = (repoId: string, path: string, side: Side) =>
  gitInvoke<WorkingStatus>("resolve_conflict_side", { repoId, path, side });

export const continueOperation = (repoId: string) =>
  gitInvoke<WorkingStatus>("continue_operation", { repoId });

export const abortOperation = (repoId: string) =>
  gitInvoke<WorkingStatus>("abort_operation", { repoId });

// ---------- rebase ----------

export type TodoAction = "pick" | "reword" | "edit" | "squash" | "fixup" | "drop";

/** One line of the list of an interactive rebase. */
export interface TodoItem {
  action: TodoAction;
  id: string;
  /** The message of the commit once done (reword, squash); `null` keeps git's own. */
  message: string | null;
}

export interface RebaseCommit {
  id: string;
  summary: string;
}

/** What rebasing onto a target would replay. */
export interface RebasePlan {
  /** The commits of the current branch not in the target, oldest first (merges are not listed). */
  commits: RebaseCommit[];
  merges: number;
  /** Commits already on the upstream: rewriting them means a force push later. */
  pushed: number;
}

export type RebaseOutcome = "upToDate" | "done" | "stopped" | "conflicts";

export interface RebaseResult {
  status: WorkingStatus;
  outcome: RebaseOutcome;
  /** The branch tip before and after: what Undo needs. */
  before: string;
  after: string;
}

export const rebasePreview = (repoId: string, onto: string) =>
  gitInvoke<RebasePlan>("rebase_preview", { repoId, onto });

/** With `items` the list of commits is the given one (interactive rebase). */
export const rebaseOnto = (
  repoId: string,
  onto: string,
  items: TodoItem[] | null,
  autostash: boolean,
) => gitInvoke<RebaseResult>("rebase_onto", { repoId, onto, items, autostash });

/** Leaves out the commit a rebase or a cherry-pick is stuck on. */
export const skipOperation = (repoId: string) =>
  gitInvoke<WorkingStatus>("skip_operation", { repoId });

// ---------- keeping status fast ----------

export interface StatusHint {
  /** How long the last `git status` took. */
  lastMs: number;
  /** git's untracked cache is already on. */
  untrackedCache: boolean;
  /** Slow, and the untracked cache would help: offer to switch it on. */
  canSpeedUp: boolean;
}

export const getStatusHint = (repoId: string) =>
  gitInvoke<StatusHint>("get_status_hint", { repoId });

/** Switches on git's untracked cache for this repository (a change to its configuration). */
export const speedUpStatus = (repoId: string) =>
  gitInvoke<StatusHint>("speed_up_status", { repoId });
