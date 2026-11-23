import { computed, ref, shallowReactive, shallowRef } from "vue";
import {
  type CommitDetail,
  type FileChange,
  type FileDiff,
  type GraphRow,
  MAX_SEARCH_RESULTS,
  type RefInfo,
  type RefKind,
} from "@/lib/api";
import { refsOfBranch } from "@/lib/branchRefs";
import { type GraphSource, type RepoBackend } from "@/lib/backend";
import type { Density } from "@/lib/metro/metrics";
import { createRecentStore, type RecentRepo, type RecentStore } from "@/lib/recents";

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
}

export type DiffMode = "unified" | "split";

const message = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

export function createWorkspace(backend: RepoBackend, recents: RecentStore) {
  const tabs = shallowRef<RepoTab[]>([]);
  const activeId = ref<string | null>(null);
  const density = ref<Density>("compact");
  const diffMode = ref<DiffMode>("unified");
  const recentRepos = ref<RecentRepo[]>(recents.list());
  /** Last error of an open attempt that did not produce a tab. */
  const openError = ref("");
  const opening = ref(false);

  const active = computed(() => tabs.value.find((tab) => tab.id === activeId.value) ?? null);

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

  async function openPath(path: string): Promise<void> {
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
        name: info.name,
        path: info.path,
        headBranch: info.headBranch,
        refs,
        hidden: new Set(),
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
      });
      tabs.value = [...tabs.value, tab];
      activeId.value = tab.id;
      recentRepos.value = recents.add({ path: info.path, name: info.name, openedAt: Date.now() });
      await loadGraph(tab);
    } catch (error) {
      openError.value = message(error);
    } finally {
      opening.value = false;
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

  const toggleRef = (tab: RepoTab, fullName: string) =>
    setHidden(tab, [fullName], !tab.hidden.has(fullName));

  /** Hides a whole kind of refs, or shows it again when everything of that kind is hidden. */
  function toggleKind(tab: RepoTab, kind: RefKind): Promise<void> {
    const names = tab.refs.filter((ref) => ref.kind === kind).map((ref) => ref.fullName);
    const allHidden = names.every((name) => tab.hidden.has(name));
    return setHidden(tab, names, !allHidden);
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

  function forgetRecent(path: string): void {
    recentRepos.value = recents.remove(path);
  }

  return {
    tabs,
    activeId,
    active,
    density,
    diffMode,
    recentRepos,
    openError,
    opening,
    openPath,
    pickAndOpen,
    closeTab,
    activate,
    toggleRef,
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
    shared = createWorkspace(backend, createRecentStore(storage));
  }
  return shared;
}
