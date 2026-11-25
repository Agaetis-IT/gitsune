import type { RepoBackend } from "@/lib/backend";
import type { GraphMeta, GraphRow, RefInfo } from "@/lib/api";
import { demoCommitDetail, demoFileDiff } from "./demoDiffs";

/**
 * Stand-in backend for plain-browser development: one recorded repository of invented
 * commits (see `dump_graph_fixture` in the Rust tests). Hidden refs are not applied.
 */
export async function createFixtureBackend(): Promise<RepoBackend> {
  const data = (await import("./graph-demo.json")) as unknown as {
    meta: GraphMeta;
    rows: GraphRow[];
  };
  const refs: RefInfo[] = data.meta.refs.map((r) => ({
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
  return {
    pickFolder: async () => info.path,
    open: async () => ({ info, refs }),
    close: async () => {},
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
    graph: async () => ({
      meta: data.meta,
      fetchPage: async (cursor, limit) => {
        const end = Math.min(data.rows.length, cursor + limit);
        return {
          start: cursor,
          rows: data.rows.slice(cursor, end),
          nextCursor: end < data.rows.length ? end : null,
          total: data.rows.length,
        };
      },
    }),
  };
}
