import type { GraphMeta, GraphRow, RefInfo } from "@/lib/api";
import type { RepoBackend } from "@/lib/backend";
import { demoCommitDetail, demoFileDiff } from "./demoDiffs";

interface Fixture {
  meta: GraphMeta;
  rows: GraphRow[];
}

/**
 * Stand-in backend for plain-browser development: one recorded repository of invented
 * commits (see `dump_graph_fixture` in the Rust tests). Hidden refs are not applied.
 *
 * `window.__simulateRepoChange()` (in the browser console) adds a commit on top of the
 * current branch and announces the change, like the file watcher of the real backend.
 */
export async function createFixtureBackend(): Promise<RepoBackend> {
  const recorded = (await import("./graph-demo.json")) as unknown as Fixture;
  let data: Fixture = structuredClone({ meta: recorded.meta, rows: recorded.rows });
  const listeners = new Set<(repoId: string) => void>();
  let simulated = 0;

  const refsOf = (fixture: Fixture): RefInfo[] =>
    fixture.meta.refs.map((r) => ({
      name: r.name,
      fullName: r.fullName,
      kind: r.kind,
      target: r.target,
      isHead: r.isHead,
    }));

  const info = {
    id: "demo-repo",
    path: "demo-repo (invented data)",
    name: "demo-repo",
    headBranch: "main",
    isBare: false,
  };

  /** A new commit on top of the current branch: every row moves down by one. */
  function addCommitOnTop(): void {
    simulated += 1;
    const old = data;
    const top = old.rows[0];
    const head = old.meta.refs.find((r) => r.isHead);
    const id = `ee${String(simulated).padStart(2, "0")}`.padEnd(40, "0");
    const newTop: GraphRow = {
      row: 0,
      id,
      parents: [top.id],
      summary: `Simulated commit ${simulated}`,
      author: "Alice Fakename",
      time: Math.floor(Date.now() / 1000),
      col: top.col,
      lane: top.lane,
      edges: [],
      refs: head
        ? [
            {
              name: head.name,
              fullName: head.fullName,
              kind: head.kind,
              target: id,
              isHead: head.isHead,
            },
          ]
        : [],
    };
    const rows = old.rows.map((r) => ({
      ...r,
      row: r.row + 1,
      // The head branch moved to the new commit.
      refs: r.refs.filter((ref) => !head || ref.fullName !== head.fullName),
    }));
    const meta: GraphMeta = {
      ...old.meta,
      total: old.meta.total + 1,
      lanes: old.meta.lanes.map((lane) => ({
        ...lane,
        firstRow: lane.id === top.lane ? 0 : lane.firstRow + 1,
        lastRow: lane.lastRow + 1,
      })),
      refs: old.meta.refs.map((r) =>
        r.isHead && head && r.fullName === head.fullName
          ? { ...r, target: id, row: 0 }
          : { ...r, row: r.row + 1 },
      ),
    };
    data = { meta, rows: [newTop, ...rows] };
  }

  if (import.meta.env.DEV) {
    (window as unknown as { __simulateRepoChange: () => void }).__simulateRepoChange = () => {
      addCommitOnTop();
      listeners.forEach((listener) => listener(info.id));
    };
  }

  return {
    pickFolder: async () => info.path,
    open: async () => ({ info, refs: refsOf(data) }),
    close: async () => {},
    refs: async () => refsOf(data),
    locate: async (_repoId, ids) => ids.map((id) => data.rows.find((r) => r.id === id)?.row ?? null),
    onRepoChanged: async (handler) => {
      listeners.add(handler);
      return () => void listeners.delete(handler);
    },
    search: async (_repoId, query, limit) => {
      const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
      if (terms.length === 0) return { rows: [], total: 0 };
      const rows = data.rows
        .filter((r) =>
          terms.every(
            (t) =>
              r.summary.toLowerCase().includes(t) ||
              r.author.toLowerCase().includes(t) ||
              r.id.startsWith(t),
          ),
        )
        .map((r) => r.row);
      return { rows: rows.slice(0, limit), total: rows.length };
    },
    commitDetail: async (_repoId, commitId) => demoCommitDetail(commitId),
    fileDiff: async (_repoId, _commitId, file, force) => demoFileDiff(file, force),
    graph: async () => {
      const current = data;
      return {
        meta: current.meta,
        fetchPage: async (cursor, limit) => {
          const end = Math.min(current.rows.length, cursor + limit);
          return {
            start: cursor,
            rows: current.rows.slice(cursor, end),
            nextCursor: end < current.rows.length ? end : null,
            total: current.rows.length,
          };
        },
      };
    },
  };
}
