import { open } from "@tauri-apps/plugin-dialog";
import {
  getCommitDetail,
  getFileDiff,
  getGraphMeta,
  type CommitDetail,
  type FileChange,
  type FileDiff,
  getGraphPage,
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
  fileDiff: (repoId, commitId, file, force) =>
    getFileDiff(repoId, commitId, file.path, file.oldPath, force),
};
