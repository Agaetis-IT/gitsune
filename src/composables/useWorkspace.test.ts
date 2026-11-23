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
  StatusEntry,
  WorkingStatus,
} from "@/lib/api";
import { GitCommandError } from "@/lib/api";
import type { GraphSource, RepoBackend } from "@/lib/backend";
import { createReadOnlyStore } from "@/lib/readOnly";
import { createRecentStore } from "@/lib/recents";
import { createWorkspace, describeOpenError } from "./useWorkspace";

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

const cleanStatus = (): WorkingStatus => ({
  branch: "main",
  detached: false,
  head: "c0",
  upstream: null,
  ahead: 0,
  behind: 0,
  staged: [],
  unstaged: [],
  conflicted: [],
});

const entry = (path: string, state: StatusEntry["state"] = "modified"): StatusEntry => ({
  path,
  oldPath: null,
  state,
  submodule: false,
});

const lastOf = <T>(items: T[]): T | undefined => items[items.length - 1];

function fakeBackend(overrides: Partial<RepoBackend> = {}) {
  const graphCalls: Array<{ id: string; hidden: string[] }> = [];
  const backend: RepoBackend = {
    pickFolder: async () => null,
    open: async (path) => ({ info: infoFor(path), refs: REFS }),
    close: vi.fn(async () => {}),
    status: async () => cleanStatus(),
    workingDiff: async () => diffFor("wip", FILE, false),
    stage: async () => cleanStatus(),
    unstage: async () => cleanStatus(),
    stageHunk: async () => cleanStatus(),
    unstageHunk: async () => cleanStatus(),
    discard: async () => ({ snapshot: null, status: cleanStatus() }),
    snapshots: async () => [],
    restoreSnapshot: async () => ({ snapshot: null, status: cleanStatus() }),
    commit: async () => ({ id: "f".repeat(40), before: "c0", status: cleanStatus() }),
    undoCommit: async () => cleanStatus(),
    identity: async () => ({ name: null, email: null }),
    setIdentity: async (_repoId, name, email) => ({ name, email }),
    setReadOnly: async () => {},
    activity: async () => [],
    clearActivity: async () => {},
    onWorktreeChanged: async () => () => {},
    onActivityChanged: async () => () => {},
    commitDetail: async (_repoId, id) => detailFor(id),
    refs: async () => REFS,
    locate: async (_repoId, ids) => ids.map(() => null),
    onRepoChanged: async () => () => {},
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
    expect(workspace.openError.value).toBe("/nope is not a git repository, or it cannot be read.");
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

// ---------- refresh when the repository changes on disk ----------

interface World {
  /** Commit ids by row, newest first. */
  ids: string[];
  refs: RefInfo[];
  /** Lane names by lane id. */
  lanes: string[];
}

const laneOf = (id: number, name: string) => ({
  id,
  col: id,
  firstRow: 0,
  lastRow: 10,
  forked: false,
  joins: false,
  name,
  kind: "branch" as const,
  color: id,
});

/** A backend whose repository can change: the graph, the refs and the row of each commit follow `world`. */
function changingRepo(world: World, overrides: Partial<RepoBackend> = {}) {
  const handlers: Array<(repoId: string) => void> = [];
  const graphCalls: string[][] = [];
  /** While `on`, graph requests wait until their release function is called. */
  const hold = { on: false, releases: [] as Array<() => void> };
  const rowFor = (row: number): GraphRow => ({ ...commitRow(row), id: world.ids[row] });
  const backend: Partial<RepoBackend> = {
    graph: (_id, hidden) => {
      graphCalls.push(hidden);
      // The graph is read when the request is answered, so it reflects the world at that time.
      const answer = (): GraphSource => ({
        meta: {
          ...emptyMeta(world.ids.length),
          lanes: world.lanes.map((name, id) => laneOf(id, name)),
        },
        fetchPage: async (cursor, limit) => ({
          start: cursor,
          rows: world.ids.slice(cursor, cursor + limit).map((_, i) => rowFor(cursor + i)),
          nextCursor: null,
          total: world.ids.length,
        }),
      });
      if (!hold.on) return Promise.resolve(answer());
      return new Promise((resolve) => hold.releases.push(() => resolve(answer())));
    },
    refs: async () => world.refs,
    locate: async (_id, wanted) =>
      wanted.map((id) => {
        const row = world.ids.indexOf(id);
        return row === -1 ? null : row;
      }),
    onRepoChanged: async (handler) => {
      handlers.push(handler);
      return () => handlers.splice(handlers.indexOf(handler), 1);
    },
    ...overrides,
  };
  const change = () => handlers.forEach((handler) => handler("/fake/a"));
  return { backend, change, graphCalls, rowFor, hold };
}

async function openChanging(world: World, overrides: Partial<RepoBackend> = {}) {
  const repo = changingRepo(world, overrides);
  const { workspace, backend } = setup(repo.backend);
  await workspace.watchRepos();
  await workspace.openPath("/fake/a");
  return { ...repo, workspace, backend, tab: workspace.tabs.value[0] };
}

const newWorld = (): World => ({
  ids: ["c0", "c1", "c2", "c3", "c4"],
  refs: REFS,
  lanes: ["main", "feature/x"],
});

describe("refreshing when the repository changes", () => {
  it("keeps the commit at the top of the view and follows the selected commit", async () => {
    const world = newWorld();
    const { workspace, tab, change, rowFor, backend } = await openChanging(world);
    workspace.select(tab, 2, rowFor(2));
    await vi.waitFor(() => expect(tab.detail?.id).toBe("c2"));
    workspace.openFile(tab, FILE);
    tab.scrollTop = 300;
    tab.topAnchor = { id: "c1", offset: 7 };
    const detailCalls = vi.spyOn(backend, "commitDetail");

    world.ids = ["n1", "n0", ...world.ids]; // two new commits on top
    change();
    await vi.waitFor(() => expect(tab.source?.meta.total).toBe(7));

    expect(tab.anchor).toEqual({ row: 3, offset: 7 }); // c1 was row 1, now row 3
    expect(tab.selected).toBe(4);
    expect(tab.selectedCommit?.id).toBe("c2");
    expect(tab.detail?.id).toBe("c2");
    expect(tab.openFile).toEqual(FILE);
    expect(detailCalls).not.toHaveBeenCalled(); // same commit: its details are kept
    expect(tab.loading).toBe(false);
    expect(tab.error).toBe("");
  });

  it("stays at the top, showing the newest commits, when the view was at the top", async () => {
    const world = newWorld();
    const { tab, change } = await openChanging(world);
    tab.scrollTop = 0;
    tab.topAnchor = { id: "c0", offset: 0 };

    world.ids = ["n0", ...world.ids];
    change();
    await vi.waitFor(() => expect(tab.source?.meta.total).toBe(6));
    expect(tab.anchor).toEqual({ row: 0, offset: 0 });
  });

  it("lets go of the selection when its commit is no longer in the graph", async () => {
    const world = newWorld();
    const { workspace, tab, change, rowFor } = await openChanging(world);
    workspace.select(tab, 3, rowFor(3));
    await vi.waitFor(() => expect(tab.detail?.id).toBe("c3"));
    workspace.openFile(tab, FILE);

    world.ids = ["c0", "c1", "c2"]; // c3 and c4 were dropped (a branch was deleted)
    change();
    await vi.waitFor(() => expect(tab.source?.meta.total).toBe(3));
    expect(tab.selected).toBeNull();
    expect(tab.selectedCommit).toBeNull();
    expect(tab.detail).toBeNull();
    expect(tab.openFile).toBeNull();
  });

  it("falls back to the selected commit when the commit at the top is gone", async () => {
    const world = newWorld();
    const { workspace, tab, change, rowFor } = await openChanging(world);
    workspace.select(tab, 3, rowFor(3));
    tab.scrollTop = 100;
    tab.topAnchor = { id: "c0", offset: 4 };

    world.ids = ["c9", "c8", "c3", "c4"]; // c0 is gone, c3 moved to row 2
    change();
    await vi.waitFor(() => expect(tab.source?.meta.total).toBe(4));
    expect(tab.anchor).toEqual({ row: 2, offset: 0 });
    expect(tab.selected).toBe(2);
  });

  it("refreshes the refs of the sidebar and keeps the hidden ones hidden", async () => {
    const world = newWorld();
    const { workspace, tab, change, graphCalls } = await openChanging(world);
    await workspace.toggleRef(tab, "refs/heads/feature/x");

    world.refs = [...REFS, ref("brand-new", "local")];
    change();
    await vi.waitFor(() => expect(tab.refs).toHaveLength(6));
    expect(tab.refs[tab.refs.length - 1]?.name).toBe("brand-new");
    expect(lastOf(graphCalls)).toEqual(["refs/heads/feature/x"]);
  });

  it("finds the highlighted branch again by name, and drops it if the branch is gone", async () => {
    const world = newWorld();
    const { workspace, tab, change } = await openChanging(world);
    workspace.toggleHighlight(tab, 1); // feature/x

    world.lanes = ["main", "other", "feature/x"]; // its lane number changed
    change();
    await vi.waitFor(() => expect(tab.source?.meta.lanes).toHaveLength(3));
    expect(tab.highlightLane).toBe(2);

    world.lanes = ["main"];
    change();
    await vi.waitFor(() => expect(tab.source?.meta.lanes).toHaveLength(1));
    expect(tab.highlightLane).toBeNull();
  });

  it("searches the same words again in the new graph", async () => {
    const world = newWorld();
    const { workspace, tab, change } = await openChanging(world, {
      search: async (_id, query) => searchFor(query),
    });
    await workspace.search(tab, "2");
    workspace.stepMatch(tab, 1);
    const before = tab.search;
    expect(before.current).toBe(1);

    change();
    await vi.waitFor(() => expect(tab.search).not.toBe(before));
    await vi.waitFor(() => expect(tab.search.loading).toBe(false));
    expect(tab.search.query).toBe("2");
    expect(tab.search.rows).toEqual([10, 20]);
  });

  it("groups changes that arrive during a refresh into one more refresh", async () => {
    const world = newWorld();
    const { tab, change, graphCalls, hold } = await openChanging(world);
    const baseline = graphCalls.length;
    hold.on = true;

    change();
    change();
    change();
    expect(graphCalls.length - baseline).toBe(1); // one refresh running, the others queued
    world.ids = ["n0", ...world.ids];
    hold.releases.shift()?.();
    await vi.waitFor(() => expect(graphCalls.length - baseline).toBe(2));
    hold.releases.shift()?.();
    await vi.waitFor(() => expect(tab.refreshing).toBe(false));
    expect(graphCalls.length - baseline).toBe(2);
    expect(tab.source?.meta.total).toBe(6);
  });

  it("ignores changes of unknown or closed repositories", async () => {
    const world = newWorld();
    const { workspace, tab, change, graphCalls } = await openChanging(world);
    const calls = graphCalls.length;
    await workspace.closeTab(tab.id);
    change();
    await Promise.resolve();
    expect(graphCalls).toHaveLength(calls);
  });

  it("reports a failed refresh and keeps showing the previous graph", async () => {
    const world = newWorld();
    let failing = false;
    const repo = changingRepo(world);
    const { workspace, backend } = setup({
      ...repo.backend,
      graph: async (id, hidden) => {
        if (failing) throw new Error("cannot read the history");
        return repo.backend.graph!(id, hidden);
      },
    });
    await workspace.watchRepos();
    await workspace.openPath("/fake/a");
    const tab = workspace.tabs.value[0];
    const before = tab.source;

    failing = true;
    repo.change();
    await vi.waitFor(() => expect(tab.error).toBe("cannot read the history"));
    expect(tab.source).toBe(before);
    expect(tab.loading).toBe(false);

    failing = false;
    repo.change();
    await vi.waitFor(() => expect(tab.error).toBe(""));
    expect(backend).toBeDefined();
  });

  it("goes back to the top when the graph is rebuilt for another reason", async () => {
    const world = newWorld();
    const { workspace, tab, change } = await openChanging(world);
    tab.scrollTop = 300;
    tab.topAnchor = { id: "c1", offset: 0 };
    change();
    await vi.waitFor(() => expect(tab.anchor).not.toBeNull());

    await workspace.toggleRef(tab, "refs/heads/feature/x");
    expect(tab.anchor).toBeNull();
    expect(tab.scrollTop).toBe(0);
  });

  it("lets a newer graph request win over a refresh still running", async () => {
    const world = newWorld();
    const { workspace, tab, change, hold } = await openChanging(world);
    hold.on = true;

    change(); // refresh (held)
    const hide = workspace.toggleRef(tab, "refs/heads/feature/x"); // newer request (held)
    hold.releases[1](); // the newer one answers while the world has 5 commits
    await hide;
    world.ids = ["n0", "n1", ...world.ids]; // 7 commits when the older one answers
    hold.releases[0]();
    await vi.waitFor(() => expect(tab.refreshing).toBe(false));
    expect(tab.source?.meta.total).toBe(5);
    expect(tab.loading).toBe(false);
  });
});

describe("error and empty states", () => {
  it("explains a folder that cannot be opened and passes other errors through", () => {
    expect(describeOpenError("/x", new Error("cannot open repository at /x: not a repo"))).toBe(
      "/x is not a git repository, or it cannot be read.",
    );
    expect(describeOpenError("/x", "disk on fire")).toBe("disk on fire");
  });

  it("shows every hidden ref again", async () => {
    const { workspace, graphCalls } = setup();
    await workspace.openPath("/fake/a");
    const tab = workspace.tabs.value[0];
    await workspace.toggleKind(tab, "tag");
    await workspace.toggleRef(tab, "refs/heads/feature/x");
    expect(tab.hidden.size).toBe(3);

    await workspace.showAllRefs(tab);
    expect(tab.hidden.size).toBe(0);
    expect(lastOf(graphCalls)?.hidden).toEqual([]);
  });

  it("retries a graph that failed to load", async () => {
    let failing = true;
    const { workspace } = setup({
      graph: async () => {
        if (failing) throw new Error("boom");
        return {
          meta: emptyMeta(10),
          fetchPage: async () => ({ start: 0, rows: [], nextCursor: null, total: 0 }),
        };
      },
    });
    await workspace.openPath("/fake/a");
    const tab = workspace.tabs.value[0];
    expect(tab.error).toBe("boom");

    failing = false;
    await workspace.retry(tab);
    expect(tab.error).toBe("");
    expect(tab.source?.meta.total).toBe(10);
  });
});

// ---------- working tree: status, staging, commits ----------

/** A backend whose working tree is a status the test controls. */
function worktreeRepo(initial: WorkingStatus = cleanStatus()) {
  const state = { status: initial };
  const calls: string[] = [];
  let worktreeHandler: ((repoId: string) => void) | undefined;
  let activityHandler: ((repoId: string) => void) | undefined;
  const backend: Partial<RepoBackend> = {
    status: async () => structuredClone(state.status),
    stage: async (_id, paths) => {
      calls.push(`stage ${paths.join(",")}`);
      state.status = {
        ...state.status,
        staged: paths.map((p) => entry(p)),
        unstaged: state.status.unstaged.filter((e) => !paths.includes(e.path)),
      };
      return structuredClone(state.status);
    },
    onWorktreeChanged: async (handler) => {
      worktreeHandler = handler;
      return () => {};
    },
    onActivityChanged: async (handler) => {
      activityHandler = handler;
      return () => {};
    },
  };
  return {
    state,
    calls,
    backend,
    touch: () => worktreeHandler?.("/fake/a"),
    activityChanged: () => activityHandler?.("/fake/a"),
  };
}

async function openWorktree(
  initial: WorkingStatus = cleanStatus(),
  overrides: Partial<RepoBackend> = {},
  readOnlyStore = createReadOnlyStore(null),
) {
  const repo = worktreeRepo(initial);
  const fake = fakeBackend({ ...repo.backend, ...overrides });
  const workspace = createWorkspace(
    fake.backend,
    createRecentStore(memoryStorage()),
    readOnlyStore,
  );
  await workspace.watchRepos();
  await workspace.openPath("/fake/a");
  const tab = workspace.tabs.value[0];
  await vi.waitFor(() => expect(tab.status).not.toBeNull());
  return { ...repo, ...fake, workspace, tab };
}

const dirty = (): WorkingStatus => ({
  ...cleanStatus(),
  unstaged: [entry("src/a.ts"), entry("src/b.ts"), entry("notes.md", "untracked")],
});

describe("working tree status", () => {
  it("is read when the repository opens and when files change", async () => {
    const { tab, state, touch } = await openWorktree(dirty());
    expect(tab.status?.unstaged).toHaveLength(3);

    state.status = { ...dirty(), unstaged: [entry("src/a.ts")] };
    touch();
    await vi.waitFor(() => expect(tab.status?.unstaged).toHaveLength(1));
  });

  it("keeps the same status object when nothing changed", async () => {
    const { tab, touch } = await openWorktree(dirty());
    const before = tab.status;
    touch();
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(tab.status).toBe(before);
  });

  it("groups changes that arrive while it is being read", async () => {
    const releases: Array<() => void> = [];
    let reads = 0;
    const { tab, touch } = await openWorktree(dirty(), {
      status: () =>
        new Promise((resolve) => {
          reads++;
          if (reads === 1) return resolve(dirty()); // opening the tab
          releases.push(() => resolve(dirty()));
        }),
    });
    const baseline = reads;
    touch();
    touch();
    touch();
    expect(reads - baseline).toBe(1);
    releases.shift()?.();
    await vi.waitFor(() => expect(reads - baseline).toBe(2));
    releases.shift()?.();
    await vi.waitFor(() => expect(tab.statusRefreshing).toBe(false));
    expect(reads - baseline).toBe(2);
  });

  it("keeps the previous status when reading fails", async () => {
    let failing = false;
    const { tab, touch } = await openWorktree(dirty(), {
      status: async () => {
        if (failing) throw new Error("no working tree");
        return dirty();
      },
    });
    failing = true;
    touch();
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(tab.status?.unstaged).toHaveLength(3);
    expect(tab.statusRefreshing).toBe(false);
  });

  it("selecting the working tree row and a commit exclude each other", async () => {
    const { workspace, tab } = await openWorktree(dirty());
    workspace.select(tab, 3, commitRow(3));
    await vi.waitFor(() => expect(tab.detail).not.toBeNull());

    workspace.selectWip(tab);
    expect(tab.wip).toBe(true);
    expect(tab.selected).toBeNull();
    expect(tab.detail).toBeNull();

    workspace.select(tab, 4, commitRow(4));
    expect(tab.wip).toBe(false);
  });
});

describe("staging", () => {
  it("applies the status the backend returns", async () => {
    const { workspace, tab, calls } = await openWorktree(dirty());
    await workspace.stage(tab, ["src/a.ts"]);
    expect(calls).toEqual(["stage src/a.ts"]);
    expect(tab.status?.staged.map((e) => e.path)).toEqual(["src/a.ts"]);
    expect(tab.busy).toBe(false);
    expect(tab.writeError).toBeNull();
  });

  it("refuses to write in read-only mode, without calling the backend", async () => {
    const { workspace, tab, calls } = await openWorktree(dirty());
    await workspace.setReadOnly(tab, true);
    await workspace.stage(tab, ["src/a.ts"]);
    expect(calls).toEqual([]);
    expect(tab.writeError?.kind).toBe("readOnly");
    expect(lastOf(workspace.notices.value)?.kind).toBe("error");

    await workspace.setReadOnly(tab, false);
    await workspace.stage(tab, ["src/a.ts"]);
    expect(calls).toEqual(["stage src/a.ts"]);
  });

  it("remembers read-only mode per repository and applies it when the repository opens again", async () => {
    const store = createReadOnlyStore(memoryStorage());
    const setReadOnly = vi.fn(async () => {});
    const first = await openWorktree(dirty(), { setReadOnly }, store);
    await first.workspace.setReadOnly(first.tab, true);
    expect(store.has("/fake/a")).toBe(true);
    setReadOnly.mockClear();

    const second = await openWorktree(dirty(), { setReadOnly }, store);
    expect(second.tab.readOnly).toBe(true);
    expect(setReadOnly).toHaveBeenCalledWith("/fake/a", true);
  });

  it("a backend that refuses (read-only) switches the tab to read-only", async () => {
    const { workspace, tab } = await openWorktree(dirty(), {
      stage: async () => {
        throw new GitCommandError("readOnly", "This repository is in read-only mode.");
      },
    });
    await workspace.stage(tab, ["src/a.ts"]);
    expect(tab.readOnly).toBe(true);
  });

  it("a stale hunk refreshes the status and tells the user", async () => {
    const { workspace, tab, state } = await openWorktree(dirty(), {
      stageHunk: async () => {
        throw new GitCommandError("staleHunk", "The file changed since this hunk was shown.");
      },
    });
    state.status = { ...dirty(), unstaged: [entry("src/a.ts")] };
    await workspace.stageHunk(tab, "src/a.ts", "h1");
    expect(tab.writeError?.kind).toBe("staleHunk");
    await vi.waitFor(() => expect(tab.status?.unstaged).toHaveLength(1));
    expect(lastOf(workspace.notices.value)?.text).toContain("changed");
  });

  it("reloads an open diff when its file changes and closes it when the file leaves the list", async () => {
    const { workspace, tab, state, touch } = await openWorktree(dirty());
    workspace.openWorkDiff(tab, entry("src/a.ts"), "unstaged");
    const version = tab.workDiffVersion;

    touch(); // same status, but the file may have been edited
    await vi.waitFor(() => expect(tab.workDiffVersion).toBeGreaterThan(version));
    expect(tab.workDiff).not.toBeNull();

    state.status = { ...dirty(), unstaged: [entry("src/b.ts")] };
    touch();
    await vi.waitFor(() => expect(tab.workDiff).toBeNull());
  });

  it("asks the backend for the diff of the open file, and only when there is one", async () => {
    const workingDiff = vi.fn(async () => diffFor("wip", FILE, false));
    const { workspace, tab } = await openWorktree(dirty(), { workingDiff });
    await expect(workspace.workingDiff(tab)).rejects.toThrow("no file selected");
    workspace.openWorkDiff(tab, entry("src/a.ts"), "staged");
    await workspace.workingDiff(tab, true);
    expect(workingDiff).toHaveBeenCalledWith("/fake/a", entry("src/a.ts"), "staged", true);
    workspace.closeWorkDiff(tab);
    expect(tab.workDiff).toBeNull();
  });
});

describe("committing", () => {
  const staged = (): WorkingStatus => ({ ...cleanStatus(), staged: [entry("src/a.ts")] });

  it("commits with the draft message, clears it, and offers to undo", async () => {
    const commit = vi.fn(async () => ({ id: "a".repeat(40), before: "c0", status: cleanStatus() }));
    const undoCommit = vi.fn(async () => staged());
    const { workspace, tab } = await openWorktree(staged(), { commit, undoCommit });
    tab.draft = { summary: "Fix the lexer", body: "Details.", amend: false, beforeAmend: null };

    await workspace.commit(tab);
    expect(commit).toHaveBeenCalledWith("/fake/a", "Fix the lexer", "Details.", false);
    expect(tab.draft.summary).toBe("");
    expect(tab.status?.staged).toEqual([]);
    const notice = lastOf(workspace.notices.value);
    expect(notice?.text).toBe("Committed aaaaaaa.");

    await notice?.action?.run();
    expect(undoCommit).toHaveBeenCalledWith("/fake/a", "c0", "a".repeat(40));
    expect(tab.status?.staged).toHaveLength(1);
  });

  it("shows hook output in the commit box instead of a notice, and keeps the draft", async () => {
    const { workspace, tab } = await openWorktree(staged(), {
      commit: async () => {
        throw new GitCommandError("hookFailed", "lint failed: trailing whitespace");
      },
    });
    tab.draft = { summary: "Rejected", body: "", amend: false, beforeAmend: null };
    const noticesBefore = workspace.notices.value.length;

    await workspace.commit(tab);
    expect(tab.writeError).toEqual({
      kind: "hookFailed",
      message: "lint failed: trailing whitespace",
    });
    expect(tab.draft.summary).toBe("Rejected");
    expect(workspace.notices.value).toHaveLength(noticesBefore);
  });

  it("asks for the identity, saves it, then commits again", async () => {
    let needIdentity = true;
    const commit = vi.fn(async () => {
      if (needIdentity) throw new GitCommandError("needsIdentity", "Author identity unknown");
      return { id: "b".repeat(40), before: "c0", status: cleanStatus() };
    });
    const setIdentity = vi.fn(async (_id: string, name: string, email: string) => {
      needIdentity = false;
      return { name, email };
    });
    const { workspace, tab } = await openWorktree(staged(), { commit, setIdentity });
    tab.draft = { summary: "Needs an author", body: "", amend: false, beforeAmend: null };

    await workspace.commit(tab);
    expect(tab.identityRequired).toBe(true);
    expect(tab.draft.summary).toBe("Needs an author");

    await workspace.submitIdentity(tab, "Alice Fakename", "alice@example.invalid", false);
    expect(setIdentity).toHaveBeenCalledWith(
      "/fake/a",
      "Alice Fakename",
      "alice@example.invalid",
      false,
    );
    expect(tab.identityRequired).toBe(false);
    expect(commit).toHaveBeenCalledTimes(2);
    expect(tab.draft.summary).toBe("");
  });

  it("cancelling the identity dialog drops the error", async () => {
    const { workspace, tab } = await openWorktree(staged(), {
      commit: async () => {
        throw new GitCommandError("needsIdentity", "Author identity unknown");
      },
    });
    tab.draft = { summary: "x", body: "", amend: false, beforeAmend: null };
    await workspace.commit(tab);
    workspace.cancelIdentity(tab);
    expect(tab.identityRequired).toBe(false);
    expect(tab.writeError).toBeNull();
  });

  it("amend fills the box with the last message and puts the draft back when unchecked", async () => {
    const commit = vi.fn(async () => ({ id: "c".repeat(40), before: "c0", status: cleanStatus() }));
    const { workspace, tab } = await openWorktree(staged(), { commit });
    tab.draft = { summary: "My draft", body: "draft body", amend: false, beforeAmend: null };

    await workspace.setAmend(tab, true);
    expect(tab.draft.amend).toBe(true);
    expect(tab.draft.summary).toBe("message of c0");
    expect(tab.draft.body).toBe("");

    await workspace.setAmend(tab, false);
    expect(tab.draft).toEqual({
      summary: "My draft",
      body: "draft body",
      amend: false,
      beforeAmend: null,
    });

    await workspace.setAmend(tab, true);
    await workspace.commit(tab);
    expect(commit).toHaveBeenCalledWith("/fake/a", "message of c0", "", true);
    expect(lastOf(workspace.notices.value)?.text).toBe("Amended ccccccc.");
  });

  it("splits the last message into summary and body when amending", async () => {
    const { workspace, tab } = await openWorktree(staged(), {
      commitDetail: async (_id, id) => ({
        ...detailFor(id),
        message: "Summary line\n\nBody line one\nBody line two",
      }),
    });
    await workspace.setAmend(tab, true);
    expect(tab.draft.summary).toBe("Summary line");
    expect(tab.draft.body).toBe("Body line one\nBody line two");
  });
});

describe("discarding", () => {
  it("offers to restore what was discarded", async () => {
    const snapshot = {
      reference: "refs/git-hydra/undo/1",
      id: "d".repeat(40),
      time: 1,
      description: "before discarding changes",
      paths: ["src/a.ts", "notes.md"],
    };
    const restoreSnapshot = vi.fn(async () => ({ snapshot: null, status: dirty() }));
    const discard = vi.fn(async () => ({ snapshot, status: cleanStatus() }));
    const { workspace, tab } = await openWorktree(dirty(), { discard, restoreSnapshot });

    await workspace.discard(tab, ["src/a.ts", "notes.md"]);
    expect(tab.status?.unstaged).toEqual([]);
    const notice = lastOf(workspace.notices.value);
    expect(notice?.text).toBe("Discarded 2 files.");

    await notice?.action?.run();
    expect(restoreSnapshot).toHaveBeenCalledWith("/fake/a", "refs/git-hydra/undo/1");
    expect(tab.status?.unstaged).toHaveLength(3);
  });

  it("says nothing when there was nothing to discard", async () => {
    const { workspace, tab } = await openWorktree(dirty());
    const before = workspace.notices.value.length;
    await workspace.discard(tab, ["ghost.txt"]);
    expect(workspace.notices.value).toHaveLength(before);
  });

  it("lists the snapshots", async () => {
    const { workspace, tab } = await openWorktree(dirty(), {
      snapshots: async () => [
        {
          reference: "refs/git-hydra/undo/2",
          id: "e".repeat(40),
          time: 2,
          description: "d",
          paths: ["a"],
        },
      ],
    });
    await workspace.loadSnapshots(tab);
    expect(tab.snapshots).toHaveLength(1);
  });
});

describe("activity log and notices", () => {
  it("loads the log, and reloads it when a command is added while the panel is open", async () => {
    const entries = [
      {
        id: 1,
        startedAt: 1,
        command: "git add -A -- a",
        durationMs: 5,
        exitCode: 0,
        ok: true,
        stdout: "",
        stderr: "",
        truncated: false,
      },
    ];
    const activity = vi.fn(async () => [...entries]);
    const { workspace, tab, activityChanged } = await openWorktree(dirty(), { activity });

    workspace.activityOpen.value = false;
    activityChanged();
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(activity).not.toHaveBeenCalled();

    workspace.activityOpen.value = true;
    activityChanged();
    await vi.waitFor(() => expect(tab.activity).toHaveLength(1));
    await workspace.clearActivity(tab);
    expect(tab.activity).toEqual([]);
  });

  it("notices disappear by themselves and can be dismissed", async () => {
    vi.useFakeTimers();
    try {
      const { workspace } = setup();
      const id = workspace.notify("info", "Hello");
      const other = workspace.notify("error", "Oops");
      expect(workspace.notices.value.map((n) => n.text)).toEqual(["Hello", "Oops"]);
      workspace.dismissNotice(other);
      expect(workspace.notices.value.map((n) => n.text)).toEqual(["Hello"]);
      vi.advanceTimersByTime(9100);
      expect(workspace.notices.value.find((n) => n.id === id)).toBeUndefined();
    } finally {
      vi.useRealTimers();
    }
  });
});
