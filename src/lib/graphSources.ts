import { getGraphMeta, getGraphPage, type GraphMeta, type GraphPage } from "@/lib/api";

/** Where the graph comes from: the Tauri backend, or a recorded fixture in browser dev mode. */
export interface GraphSource {
  meta: GraphMeta;
  fetchPage: (cursor: number, limit: number) => Promise<GraphPage>;
}

export async function tauriGraphSource(repoId: string): Promise<GraphSource> {
  const meta = await getGraphMeta(repoId);
  return { meta, fetchPage: (cursor, limit) => getGraphPage(repoId, cursor, limit) };
}

export const isTauri = (): boolean => "__TAURI_INTERNALS__" in window;
