import type { GraphMeta, GraphRow } from "@/lib/api";
import type { GraphSource } from "@/lib/graphSources";

/** Graph of an invented 500-commit repository, recorded from the Rust backend (dev only). */
export async function fixtureGraphSource(): Promise<GraphSource> {
  const data = (await import("./graph-demo.json")) as unknown as {
    meta: GraphMeta;
    rows: GraphRow[];
  };
  return {
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
  };
}
