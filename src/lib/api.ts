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

export interface GraphMeta {
  /** Number of rows (commits). */
  total: number;
  maxCols: number;
  lanes: Lane[];
  refs: RefInfo[];
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

/** Largest page the backend returns. */
export const MAX_PAGE_SIZE = 1000;

export const openRepo = (path: string) => invoke<RepoInfo>("open_repo", { path });
export const closeRepo = (repoId: string) => invoke<void>("close_repo", { repoId });
export const listRefs = (repoId: string) => invoke<RefInfo[]>("list_refs", { repoId });

/** Reads and lays out the whole graph; call it before requesting pages. */
export const getGraphMeta = (repoId: string) => invoke<GraphMeta>("get_graph_meta", { repoId });

export const getGraphPage = (repoId: string, cursor: number, limit: number) =>
  invoke<GraphPage>("get_graph_page", { repoId, cursor, limit });
