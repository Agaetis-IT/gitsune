import { describe, expect, it, vi } from "vitest";
import type { GraphPage, GraphRow } from "@/lib/api";
import { RowCache } from "./rowCache";

const TOTAL = 1000;

const makeRow = (row: number): GraphRow => ({
  row,
  id: `id-${row}`,
  parents: [],
  summary: `commit ${row}`,
  author: "Alice Fakename",
  time: 1_577_836_800 + row,
  col: 0,
  lane: 0,
  edges: [],
  refs: [],
});

function fakeSource() {
  const calls: Array<[number, number]> = [];
  const pending: Array<() => void> = [];
  const fetchPage = vi.fn((cursor: number, limit: number) => {
    calls.push([cursor, limit]);
    return new Promise<GraphPage>((resolve) => {
      pending.push(() => {
        const end = Math.min(TOTAL, cursor + limit);
        resolve({
          start: cursor,
          rows: Array.from({ length: end - cursor }, (_, i) => makeRow(cursor + i)),
          nextCursor: end < TOTAL ? end : null,
          total: TOTAL,
        });
      });
    });
  });
  const settle = async () => {
    pending.splice(0).forEach((resolve) => resolve());
    await Promise.resolve();
    await Promise.resolve();
  };
  return { fetchPage, calls, settle };
}

describe("RowCache", () => {
  it("loads the requested chunk and its neighbours, once", async () => {
    const source = fakeSource();
    const onUpdate = vi.fn();
    const cache = new RowCache({ fetchPage: source.fetchPage, onUpdate, chunkSize: 100 });

    cache.ensure(250, 260, TOTAL);
    cache.ensure(250, 260, TOTAL); // repeated while in flight: no new request
    expect(source.calls).toEqual([
      [100, 100],
      [200, 100],
      [300, 100],
    ]);
    expect(cache.getRow(255)).toBeUndefined();

    await source.settle();
    expect(cache.getRow(255)?.summary).toBe("commit 255");
    expect(cache.getRow(0)).toBeUndefined();
    expect(onUpdate).toHaveBeenCalledTimes(3);

    cache.ensure(250, 260, TOTAL); // already loaded: still nothing new
    expect(source.calls).toHaveLength(3);
  });

  it("tells whether a range of rows is loaded", async () => {
    const source = fakeSource();
    const cache = new RowCache({
      fetchPage: source.fetchPage,
      onUpdate: () => {},
      chunkSize: 100,
      prefetch: 0,
    });
    expect(cache.has(10, 20)).toBe(false);
    cache.ensure(10, 20, TOTAL);
    await source.settle();
    expect(cache.has(10, 20)).toBe(true);
    expect(cache.has(0, 99)).toBe(true);
    expect(cache.has(90, 110)).toBe(false); // runs into the next chunk
    cache.reset();
    expect(cache.has(10, 20)).toBe(false);
  });

  it("does not ask for chunks beyond the graph", () => {
    const source = fakeSource();
    const cache = new RowCache({ fetchPage: source.fetchPage, onUpdate: () => {}, chunkSize: 100 });
    cache.ensure(990, 999, TOTAL);
    expect(source.calls.map(([cursor]) => cursor)).toEqual([800, 900]);
    cache.ensure(0, 0, 0);
    expect(source.calls).toHaveLength(2);
  });

  it("drops the chunks that were needed the longest ago", async () => {
    const source = fakeSource();
    const cache = new RowCache({
      fetchPage: source.fetchPage,
      onUpdate: () => {},
      chunkSize: 100,
      maxChunks: 3,
      prefetch: 0,
    });
    for (const row of [0, 100, 200]) {
      cache.ensure(row, row, TOTAL);
      await source.settle();
    }
    cache.ensure(0, 0, TOTAL); // chunk 0 becomes the most recently needed
    cache.ensure(300, 300, TOTAL);
    await source.settle();

    expect(cache.getRow(0)).toBeDefined();
    expect(cache.getRow(100)).toBeUndefined(); // evicted
    expect(cache.getRow(200)).toBeDefined();
    expect(cache.getRow(300)).toBeDefined();
  });

  it("ignores responses that arrive after a reset", async () => {
    const source = fakeSource();
    const onUpdate = vi.fn();
    const cache = new RowCache({
      fetchPage: source.fetchPage,
      onUpdate,
      chunkSize: 100,
      prefetch: 0,
    });
    cache.ensure(0, 0, TOTAL);
    cache.reset();
    await source.settle();
    expect(cache.getRow(0)).toBeUndefined();
    expect(onUpdate).not.toHaveBeenCalled();

    cache.ensure(0, 0, TOTAL); // fetched again after the reset
    expect(source.calls).toHaveLength(2);
  });

  it("reports failures once and does not retry in a loop", async () => {
    const onError = vi.fn();
    const fetchPage = vi.fn(() => Promise.reject(new Error("boom")));
    const cache = new RowCache({
      fetchPage,
      onUpdate: () => {},
      onError,
      chunkSize: 100,
      prefetch: 0,
    });
    cache.ensure(0, 0, TOTAL);
    await Promise.resolve();
    await Promise.resolve();
    cache.ensure(0, 0, TOTAL);
    expect(fetchPage).toHaveBeenCalledTimes(1);
    expect(onError).toHaveBeenCalledTimes(1);

    cache.reset(); // a reset allows trying again
    cache.ensure(0, 0, TOTAL);
    expect(fetchPage).toHaveBeenCalledTimes(2);
  });
});
