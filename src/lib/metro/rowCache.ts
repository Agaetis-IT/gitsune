import type { GraphPage, GraphRow } from "@/lib/api";

export type FetchPage = (cursor: number, limit: number) => Promise<GraphPage>;

export interface RowCacheOptions {
  fetchPage: FetchPage;
  /** Called when new rows arrive, so the view can redraw. */
  onUpdate: () => void;
  onError?: (error: unknown) => void;
  /** Rows per request (and per cache chunk). */
  chunkSize?: number;
  /** Chunks kept in memory; the least recently needed ones are dropped first. */
  maxChunks?: number;
  /** Chunks fetched ahead of and behind the requested range. */
  prefetch?: number;
}

/**
 * Sparse, bounded cache of graph rows, loaded by chunks as the view scrolls.
 * Requests for chunks already loaded or in flight are not repeated.
 */
export class RowCache {
  private readonly chunks = new Map<number, GraphRow[]>();
  private readonly inFlight = new Set<number>();
  private readonly failed = new Set<number>();
  private readonly lastUsed = new Map<number, number>();
  private tick = 0;
  private generation = 0;
  private readonly chunkSize: number;
  private readonly maxChunks: number;
  private readonly prefetch: number;

  constructor(private readonly options: RowCacheOptions) {
    this.chunkSize = options.chunkSize ?? 200;
    this.maxChunks = options.maxChunks ?? 30;
    this.prefetch = options.prefetch ?? 1;
  }

  getRow(row: number): GraphRow | undefined {
    return this.chunks.get(Math.floor(row / this.chunkSize))?.[row % this.chunkSize];
  }

  /** Makes sure rows `first..=last` (and a margin around them) are loaded or loading. */
  ensure(first: number, last: number, total: number): void {
    if (total === 0) return;
    const lastChunk = Math.floor((total - 1) / this.chunkSize);
    const from = Math.max(0, Math.floor(first / this.chunkSize) - this.prefetch);
    const to = Math.min(lastChunk, Math.floor(last / this.chunkSize) + this.prefetch);
    this.tick += 1;
    for (let chunk = from; chunk <= to; chunk++) {
      this.lastUsed.set(chunk, this.tick);
      if (!this.chunks.has(chunk) && !this.inFlight.has(chunk) && !this.failed.has(chunk)) {
        void this.load(chunk);
      }
    }
  }

  /** Forgets everything (new repository, new layout). Responses still in flight are ignored. */
  reset(): void {
    this.generation += 1;
    this.chunks.clear();
    this.inFlight.clear();
    this.failed.clear();
    this.lastUsed.clear();
  }

  private async load(chunk: number): Promise<void> {
    const generation = this.generation;
    this.inFlight.add(chunk);
    try {
      const page = await this.options.fetchPage(chunk * this.chunkSize, this.chunkSize);
      if (generation !== this.generation) return;
      this.chunks.set(chunk, page.rows);
      this.evict();
      this.options.onUpdate();
    } catch (error) {
      if (generation !== this.generation) return;
      this.failed.add(chunk);
      this.options.onError?.(error);
    } finally {
      if (generation === this.generation) this.inFlight.delete(chunk);
    }
  }

  private evict(): void {
    while (this.chunks.size > this.maxChunks) {
      let oldest: number | undefined;
      for (const chunk of this.chunks.keys()) {
        if (
          oldest === undefined ||
          (this.lastUsed.get(chunk) ?? 0) < (this.lastUsed.get(oldest) ?? 0)
        ) {
          oldest = chunk;
        }
      }
      if (oldest === undefined) return;
      this.chunks.delete(oldest);
    }
  }
}
