import type { RepoBackend } from "@/lib/backend";
import type { GraphMeta, GraphRow, RefInfo } from "@/lib/api";

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
