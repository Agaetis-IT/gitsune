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
