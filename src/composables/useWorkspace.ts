import { computed, ref, shallowReactive, shallowRef } from "vue";
import type { GraphRow, RefInfo, RefKind } from "@/lib/api";
import { type GraphSource, type RepoBackend } from "@/lib/backend";
import type { Density } from "@/lib/metro/metrics";
import { createRecentStore, type RecentRepo, type RecentStore } from "@/lib/recents";

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
}

const message = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

export function createWorkspace(backend: RepoBackend, recents: RecentStore) {
  const tabs = shallowRef<RepoTab[]>([]);
  const activeId = ref<string | null>(null);
  const density = ref<Density>("compact");
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
      tab.selected = null;
      tab.selectedCommit = null;
      tab.scrollTop = 0;
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

  function select(tab: RepoTab, row: number | null, commit: GraphRow | null = null): void {
    tab.selected = row;
    tab.selectedCommit = row === null ? null : commit;
  }

  function forgetRecent(path: string): void {
    recentRepos.value = recents.remove(path);
  }

  return {
    tabs,
    activeId,
    active,
    density,
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
