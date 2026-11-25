import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
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
  commitDetail: getCommitDetail,
  search: searchCommits,
  refs: listRefs,
  locate: locateCommits,
  onRepoChanged: (handler) =>
    listen<{ repoId: string }>("repo-changed", (event) => handler(event.payload.repoId)),
  fileDiff: (repoId, commitId, file, force) =>
    getFileDiff(repoId, commitId, file.path, file.oldPath, force),
};
