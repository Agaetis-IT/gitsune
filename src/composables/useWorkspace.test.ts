import { describe, expect, it, vi } from "vitest";
import type {
  CommitDetail,
  FileChange,
  FileDiff,
  GraphMeta,
  GraphRow,
  RefInfo,
  RepoInfo,
  SearchResult,
} from "@/lib/api";
import type { GraphSource, RepoBackend } from "@/lib/backend";
import { createRecentStore } from "@/lib/recents";
import { createWorkspace } from "./useWorkspace";

const ref = (name: string, kind: RefInfo["kind"], isHead = false): RefInfo => ({
  name,
  fullName: `refs/${kind === "local" ? "heads" : kind === "remote" ? "remotes" : "tags"}/${name}`,
  kind,
  target: "0".repeat(40),
  isHead,
});

const REFS = [
  ref("main", "local", true),
  ref("feature/x", "local"),
  ref("origin/main", "remote"),
  ref("v1", "tag"),
  ref("v2", "tag"),
];

const infoFor = (path: string): RepoInfo => ({
  id: path,
  path,
  name: path.split("/").pop() ?? path,
  headBranch: "main",
  isBare: false,
});

const emptyMeta = (total: number): GraphMeta => ({
  total,
  maxCols: 1,
  lanes: [],
  refs: [],
  loadMs: 0,
  layoutMs: 0,
});

const detailFor = (id: string): CommitDetail => ({
  id,
  parents: [],
  authorName: "Alice Fakename",
  authorEmail: "alice@example.invalid",
  authorTime: 1_577_836_800,
  message: `message of ${id}`,
  files: [FILE],
  filesTruncated: false,
});

const diffFor = (_id: string, file: FileChange, force: boolean): FileDiff => ({
  path: file.path,
  oldPath: file.oldPath,
  status: file.status,
  binary: false,
  submodule: false,
  tooLarge: !force,
  canForce: true,
  truncated: false,
  oldSize: null,
  newSize: null,
  additions: 0,
  deletions: 0,
  hunks: [],
});

const FILE: FileChange = {
  path: "src/a.ts",
  oldPath: null,
  status: "modified",
  additions: 1,
  deletions: 1,
  binary: false,
};

const commitRow = (row: number): GraphRow => ({
  row,
  id: `id-${row}`,
  parents: [],
  summary: `commit ${row}`,
  author: "Alice Fakename",
  time: 1_577_836_800,
  col: 0,
  lane: 0,
  edges: [],
  refs: [],
});

/** Fake search: the query `n` matches rows 1..n, anything else matches nothing. */
const searchFor = (query: string): SearchResult => {
  const n = Number(query);
  const rows =
    Number.isInteger(n) && n > 0 ? Array.from({ length: n }, (_, i) => (i + 1) * 10) : [];
  return { rows, total: rows.length };
};

const lastOf = <T>(items: T[]): T | undefined => items[items.length - 1];

function fakeBackend(overrides: Partial<RepoBackend> = {}) {
  const graphCalls: Array<{ id: string; hidden: string[] }> = [];
  const backend: RepoBackend = {
    pickFolder: async () => null,
    open: async (path) => ({ info: infoFor(path), refs: REFS }),
    close: vi.fn(async () => {}),
    commitDetail: async (_repoId, id) => detailFor(id),
    search: async (_repoId, query) => searchFor(query),
    fileDiff: async (_repoId, id, file, force) => diffFor(id, file, force),
    graph: async (id, hidden) => {
      graphCalls.push({ id, hidden });
      const source: GraphSource = {
        meta: emptyMeta(100 - hidden.length),
        fetchPage: async () => ({ start: 0, rows: [], nextCursor: null, total: 0 }),
      };
      return source;
    },
    ...overrides,
  };
  return { backend, graphCalls };
}

function memoryStorage() {
  const data = new Map<string, string>();
  return {
    getItem: (key: string) => data.get(key) ?? null,
    setItem: (key: string, value: string) => void data.set(key, value),
  };
}

function setup(overrides: Partial<RepoBackend> = {}) {
  const fake = fakeBackend(overrides);
  const workspace = createWorkspace(fake.backend, createRecentStore(memoryStorage()));
  return { ...fake, workspace };
}

describe("opening repositories", () => {
  it("opens a tab, loads its graph, activates it and remembers it", async () => {
    const { workspace, graphCalls } = setup();
    await workspace.openPath("/fake/alpha");

    expect(workspace.tabs.value.map((t) => t.name)).toEqual(["alpha"]);
    expect(workspace.active.value?.id).toBe("/fake/alpha");
    expect(workspace.active.value?.source?.meta.total).toBe(100);
    expect(workspace.active.value?.refs).toHaveLength(5);
    expect(workspace.active.value?.loading).toBe(false);
    expect(graphCalls).toEqual([{ id: "/fake/alpha", hidden: [] }]);
    expect(workspace.recentRepos.value.map((r) => r.name)).toEqual(["alpha"]);
  });

  it("activates the existing tab instead of opening the same repository twice", async () => {
    const { workspace, graphCalls } = setup();
    await workspace.openPath("/fake/alpha");
    await workspace.openPath("/fake/beta");
    await workspace.openPath("/fake/alpha");

    expect(workspace.tabs.value).toHaveLength(2);
    expect(workspace.activeId.value).toBe("/fake/alpha");
    expect(graphCalls).toHaveLength(2);
  });

  it("reports a failure without creating a tab", async () => {
    const { workspace } = setup({
      open: async () => {
        throw new Error("cannot open repository at /nope");
      },
    });
    await workspace.openPath("/nope");
    expect(workspace.tabs.value).toHaveLength(0);
    expect(workspace.openError.value).toContain("cannot open repository");
    expect(workspace.recentRepos.value).toEqual([]);
    expect(workspace.opening.value).toBe(false);
  });

  it("ignores an empty path and a cancelled folder picker", async () => {
    const open = vi.fn();
    const { workspace } = setup({ open, pickFolder: async () => null });
    await workspace.openPath("   ");
    await workspace.pickAndOpen();
    expect(open).not.toHaveBeenCalled();
  });

  it("opens the folder chosen in the picker", async () => {
    const { workspace } = setup({ pickFolder: async () => "/fake/picked" });
    await workspace.pickAndOpen();
    expect(workspace.active.value?.name).toBe("picked");
  });

  it("keeps the tab with an error when its graph cannot be built", async () => {
    const { workspace } = setup({
      graph: async () => {
        throw new Error("boom");
      },
    });
    await workspace.openPath("/fake/alpha");
    expect(workspace.active.value?.error).toBe("boom");
    expect(workspace.active.value?.source).toBeNull();
    expect(workspace.active.value?.loading).toBe(false);
  });
});

describe("tabs", () => {
  it("activates a neighbour when the active tab is closed", async () => {
    const { workspace, backend } = setup();
    for (const name of ["a", "b", "c"]) await workspace.openPath(`/fake/${name}`);
    workspace.activate("/fake/b");

    await workspace.closeTab("/fake/b");
    expect(workspace.activeId.value).toBe("/fake/c");
    await workspace.closeTab("/fake/c");
    expect(workspace.activeId.value).toBe("/fake/a");
    await workspace.closeTab("/fake/a");
    expect(workspace.activeId.value).toBeNull();
    expect(workspace.tabs.value).toHaveLength(0);
    expect(backend.close).toHaveBeenCalledTimes(3);
  });

  it("keeps the active tab when another one is closed, and ignores unknown ids", async () => {
    const { workspace, backend } = setup();
    await workspace.openPath("/fake/a");
    await workspace.openPath("/fake/b");
    await workspace.closeTab("/fake/a");
    await workspace.closeTab("/fake/zzz");
    expect(workspace.activeId.value).toBe("/fake/b");
    expect(backend.close).toHaveBeenCalledTimes(1);
    workspace.activate("/fake/zzz");
    expect(workspace.activeId.value).toBe("/fake/b");
  });

  it("closes the tab even if the backend fails to close the repository", async () => {
    const { workspace } = setup({
      close: async () => {
        throw new Error("already closed");
      },
    });
    await workspace.openPath("/fake/a");
    await workspace.closeTab("/fake/a");
    expect(workspace.tabs.value).toHaveLength(0);
  });

  it("remembers selection and scroll per tab", async () => {
    const { workspace } = setup();
    await workspace.openPath("/fake/a");
    await workspace.openPath("/fake/b");
    const [a, b] = workspace.tabs.value;
    workspace.select(a, 7);
    a.scrollTop = 420;
    workspace.activate(b.id);
    workspace.activate(a.id);
    expect(workspace.active.value?.selected).toBe(7);
    expect(workspace.active.value?.scrollTop).toBe(420);
    expect(b.selected).toBeNull();
  });
});

describe("hiding refs", () => {
  it("hides a ref and rebuilds the graph without it, then shows it again", async () => {
    const { workspace, graphCalls } = setup();
    await workspace.openPath("/fake/a");
    const tab = workspace.tabs.value[0];
    workspace.select(tab, 3);

    await workspace.toggleRef(tab, "refs/heads/feature/x");
    expect(tab.hidden.has("refs/heads/feature/x")).toBe(true);
    expect(lastOf(graphCalls)?.hidden).toEqual(["refs/heads/feature/x"]);
    expect(tab.source?.meta.total).toBe(99);
    expect(tab.selected).toBeNull(); // rows changed: the selection is dropped

    await workspace.toggleRef(tab, "refs/heads/feature/x");
    expect(tab.hidden.size).toBe(0);
    expect(lastOf(graphCalls)?.hidden).toEqual([]);
  });

  it("hides and shows a whole kind of refs", async () => {
    const { workspace, graphCalls } = setup();
    await workspace.openPath("/fake/a");
    const tab = workspace.tabs.value[0];

    await workspace.toggleKind(tab, "tag");
    expect(lastOf(graphCalls)?.hidden.sort()).toEqual(["refs/tags/v1", "refs/tags/v2"]);
    await workspace.toggleKind(tab, "tag");
    expect(tab.hidden.size).toBe(0);

    // Partly hidden: hiding the kind finishes the job.
    await workspace.toggleRef(tab, "refs/tags/v1");
    await workspace.toggleKind(tab, "tag");
    expect(tab.hidden.size).toBe(2);
  });

  it("ignores a graph answer that arrives after a newer request", async () => {
    const resolvers: Array<() => void> = [];
    const { workspace } = setup({
      graph: (id, hidden) =>
        new Promise((resolve) => {
          const call = () =>
            resolve({
              meta: emptyMeta(100 - hidden.length),
              fetchPage: async () => ({ start: 0, rows: [], nextCursor: null, total: 0 }),
            });
          // The first request (opening the tab) answers immediately.
          if (resolvers.length === 0 && hidden.length === 0 && id) call();
          else resolvers.push(call);
        }),
    });
    await workspace.openPath("/fake/a");
    const tab = workspace.tabs.value[0];

    const first = workspace.toggleRef(tab, "refs/heads/feature/x"); // 99 commits
    const second = workspace.toggleRef(tab, "refs/tags/v1"); // 98 commits
    resolvers[1](); // the newest answers first
    resolvers[0](); // the stale one answers last and must be dropped
    await Promise.all([first, second]);

    expect(tab.source?.meta.total).toBe(98);
    expect(tab.loading).toBe(false);
  });
});

describe("commit details and open file", () => {
  it("loads the details of the selected commit", async () => {
    const { workspace } = setup();
    await workspace.openPath("/fake/a");
    const tab = workspace.tabs.value[0];

    workspace.select(tab, 3); // the row is selected before its commit is known
    expect(tab.detail).toBeNull();
    workspace.select(tab, 3, commitRow(3));
    expect(tab.detailLoading).toBe(true);
    await vi.waitFor(() => expect(tab.detail?.id).toBe("id-3"));
    expect(tab.detail?.message).toBe("message of id-3");
    expect(tab.detailLoading).toBe(false);
    expect(tab.detailError).toBe("");
  });

  it("does not reload when the same commit is selected again", async () => {
    const commitDetail = vi.fn(async (_repoId: string, id: string) => detailFor(id));
    const { workspace } = setup({ commitDetail });
    await workspace.openPath("/fake/a");
    const tab = workspace.tabs.value[0];

    workspace.select(tab, 3, commitRow(3));
    await vi.waitFor(() => expect(tab.detail).not.toBeNull());
    workspace.openFile(tab, FILE);
    workspace.select(tab, 3); // same row, no commit: nothing changes
    workspace.select(tab, 3, commitRow(3));
    expect(commitDetail).toHaveBeenCalledTimes(1);
    expect(tab.openFile).toEqual(FILE);
  });

  it("clears the details and closes the diff when another commit is selected", async () => {
    const { workspace } = setup();
    await workspace.openPath("/fake/a");
    const tab = workspace.tabs.value[0];
    workspace.select(tab, 3, commitRow(3));
    await vi.waitFor(() => expect(tab.detail).not.toBeNull());
    workspace.openFile(tab, FILE);

    workspace.select(tab, 4);
    expect(tab.detail).toBeNull();
    expect(tab.openFile).toBeNull();
    expect(tab.selectedCommit).toBeNull();

    workspace.select(tab, null);
    expect(tab.selected).toBeNull();
  });

  it("ignores details that arrive after another commit was selected", async () => {
    const answers = new Map<string, (detail: CommitDetail) => void>();
    const { workspace } = setup({
      commitDetail: (_repoId, id) => new Promise((resolve) => answers.set(id, resolve)),
    });
    await workspace.openPath("/fake/a");
    const tab = workspace.tabs.value[0];

    workspace.select(tab, 1, commitRow(1));
    workspace.select(tab, 2, commitRow(2));
    answers.get("id-2")?.(detailFor("id-2"));
    await vi.waitFor(() => expect(tab.detail?.id).toBe("id-2"));
    answers.get("id-1")?.(detailFor("id-1")); // late answer for the first commit
    await Promise.resolve();
    expect(tab.detail?.id).toBe("id-2");
    expect(tab.detailLoading).toBe(false);
  });

  it("reports a failure to read the details", async () => {
    const { workspace } = setup({
      commitDetail: async () => {
        throw new Error("cannot read commit");
      },
    });
    await workspace.openPath("/fake/a");
    const tab = workspace.tabs.value[0];
    workspace.select(tab, 1, commitRow(1));
    await vi.waitFor(() => expect(tab.detailError).toBe("cannot read commit"));
    expect(tab.detail).toBeNull();
    expect(tab.detailLoading).toBe(false);
  });

  it("asks the backend for the diff of the open file, and only when there is one", async () => {
    const fileDiff = vi.fn(async (_r: string, id: string, file: FileChange, force: boolean) =>
      diffFor(id, file, force),
    );
    const { workspace } = setup({ fileDiff });
    await workspace.openPath("/fake/a");
    const tab = workspace.tabs.value[0];

    await expect(workspace.fileDiff(tab)).rejects.toThrow("no file selected");
    workspace.select(tab, 5, commitRow(5));
    workspace.openFile(tab, FILE);
    const diff = await workspace.fileDiff(tab, true);
    expect(diff.tooLarge).toBe(false);
    expect(fileDiff).toHaveBeenCalledWith("/fake/a", "id-5", FILE, true);

    workspace.openFile(tab, null);
    expect(tab.openFile).toBeNull();
  });

  it("drops the selection when the graph is rebuilt", async () => {
    const { workspace } = setup();
    await workspace.openPath("/fake/a");
    const tab = workspace.tabs.value[0];
    workspace.select(tab, 3, commitRow(3));
    await vi.waitFor(() => expect(tab.detail).not.toBeNull());
    workspace.openFile(tab, FILE);

    await workspace.toggleRef(tab, "refs/heads/feature/x");
    expect(tab.selected).toBeNull();
    expect(tab.detail).toBeNull();
    expect(tab.openFile).toBeNull();
  });
});

describe("search", () => {
  it("stores the matches and starts on the first one", async () => {
    const { workspace } = setup();
    await workspace.openPath("/fake/a");
    const tab = workspace.tabs.value[0];

    await workspace.search(tab, "3");
    expect(tab.search.rows).toEqual([10, 20, 30]);
    expect(tab.search.total).toBe(3);
    expect(tab.search.current).toBe(0);
    expect(tab.search.loading).toBe(false);
    expect(tab.search.query).toBe("3");
  });

  it("steps through the matches in both directions and wraps around", async () => {
    const { workspace } = setup();
    await workspace.openPath("/fake/a");
    const tab = workspace.tabs.value[0];
    await workspace.search(tab, "3");

    expect(workspace.stepMatch(tab, 1)).toBe(20);
    expect(workspace.stepMatch(tab, 1)).toBe(30);
    expect(workspace.stepMatch(tab, 1)).toBe(10);
    expect(workspace.stepMatch(tab, -1)).toBe(30);
    expect(tab.search.current).toBe(2);
  });

  it("has nothing to step through without matches", async () => {
    const { workspace } = setup();
    await workspace.openPath("/fake/a");
    const tab = workspace.tabs.value[0];
    await workspace.search(tab, "no match");
    expect(tab.search.rows).toEqual([]);
    expect(tab.search.current).toBe(-1);
    expect(workspace.stepMatch(tab, 1)).toBeNull();
  });

  it("clears the results for a blank query and with clearSearch", async () => {
    const { workspace } = setup();
    await workspace.openPath("/fake/a");
    const tab = workspace.tabs.value[0];
    await workspace.search(tab, "2");
    await workspace.search(tab, "   ");
    expect(tab.search.rows).toEqual([]);
    expect(tab.search.query).toBe("");

    await workspace.search(tab, "2");
    workspace.clearSearch(tab);
    expect(tab.search.rows).toEqual([]);
    expect(tab.search.query).toBe("");
  });

  it("ignores an answer that arrives after a newer search", async () => {
    const answers: Array<() => void> = [];
    const { workspace } = setup({
      search: (_repoId, query) =>
        new Promise((resolve) => answers.push(() => resolve(searchFor(query)))),
    });
    await workspace.openPath("/fake/a");
    const tab = workspace.tabs.value[0];

    const first = workspace.search(tab, "1");
    const second = workspace.search(tab, "2");
    answers[1](); // the newest answers first
    answers[0](); // the stale one must not overwrite it
    await Promise.all([first, second]);
    expect(tab.search.rows).toEqual([10, 20]);
    expect(tab.search.query).toBe("2");

    // Clearing while a search is running also drops its answer.
    const pending = workspace.search(tab, "3");
    workspace.clearSearch(tab);
    answers[2]();
    await pending;
    expect(tab.search.rows).toEqual([]);
  });

  it("reports a failed search", async () => {
    const { workspace } = setup({
      search: async () => {
        throw new Error("index unavailable");
      },
    });
    await workspace.openPath("/fake/a");
    const tab = workspace.tabs.value[0];
    await workspace.search(tab, "x");
    expect(tab.search.error).toBe("index unavailable");
    expect(tab.search.loading).toBe(false);
    expect(tab.search.rows).toEqual([]);
  });

  it("runs the same words again when the graph is rebuilt", async () => {
    const { workspace } = setup();
    await workspace.openPath("/fake/a");
    const tab = workspace.tabs.value[0];
    await workspace.search(tab, "2");
    tab.highlightLane = 4;

    await workspace.toggleRef(tab, "refs/heads/feature/x");
    expect(tab.highlightLane).toBeNull(); // lane ids changed
    await vi.waitFor(() => expect(tab.search.rows).toEqual([10, 20]));
    expect(tab.search.query).toBe("2");
  });
});

describe("highlight and branch hiding", () => {
  it("toggles the highlighted lane and clears it", async () => {
    const { workspace } = setup();
    await workspace.openPath("/fake/a");
    const tab = workspace.tabs.value[0];

    workspace.toggleHighlight(tab, 3);
    expect(tab.highlightLane).toBe(3);
    workspace.toggleHighlight(tab, 5);
    expect(tab.highlightLane).toBe(5);
    workspace.toggleHighlight(tab, 5);
    expect(tab.highlightLane).toBeNull();
    workspace.toggleHighlight(tab, 1);
    workspace.clearHighlight(tab);
    expect(tab.highlightLane).toBeNull();
  });

  it("hides the local and remote refs of a branch, and nothing for an unknown lane", async () => {
    const { workspace, graphCalls } = setup();
    await workspace.openPath("/fake/a");
    const tab = workspace.tabs.value[0];

    await workspace.hideBranch(tab, "origin/main");
    expect([...tab.hidden].sort()).toEqual(["refs/heads/main", "refs/remotes/origin/main"]);
    expect(lastOf(graphCalls)?.hidden.sort()).toEqual([
      "refs/heads/main",
      "refs/remotes/origin/main",
    ]);

    const calls = graphCalls.length;
    await workspace.hideBranch(tab, "no such lane");
    expect(graphCalls).toHaveLength(calls);
  });
});
