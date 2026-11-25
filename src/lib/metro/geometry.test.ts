import { describe, expect, it } from "vitest";
import type { Edge, Lane } from "@/lib/api";
import {
  connectorPath,
  isOnStation,
  laneAt,
  laneVerticalRange,
  rowAtY,
  visibleRows,
} from "./geometry";
import { colX, graphContentWidth, METRICS, rowCenterY } from "./metrics";

const m = METRICS.compact; // rowH 28, colW 18, graphPad 14, cornerR 9

const lane = (overrides: Partial<Lane>): Lane => ({
  id: 0,
  col: 0,
  firstRow: 10,
  lastRow: 20,
  forked: false,
  joins: false,
  name: null,
  kind: "anonymous",
  color: null,
  ...overrides,
});

describe("connectorPath", () => {
  it("leaves a merge commit sideways, then turns down into the new lane", () => {
    const edge: Edge = { lane: 1, fromCol: 0, toCol: 1, bend: "start" };
    expect(connectorPath(edge, 100, m)).toEqual([
      ["M", 14, 100],
      ["L", 23, 100],
      ["Q", 32, 100, 32, 109],
      ["L", 32, 128],
    ]);
  });

  it("runs down its lane, then turns toward the commit it joins", () => {
    const edge: Edge = { lane: 1, fromCol: 1, toCol: 0, bend: "end" };
    expect(connectorPath(edge, 100, m)).toEqual([
      ["M", 32, 100],
      ["L", 32, 119],
      ["Q", 32, 128, 23, 128],
      ["L", 14, 128],
    ]);
  });

  it("keeps corners within the span for far columns and tall radii", () => {
    const edge: Edge = { lane: 2, fromCol: 0, toCol: 5, bend: "start" };
    const path = connectorPath(edge, 0, m);
    expect(path[1]).toEqual(["L", colX(5, m) - 9, 0]);
    expect(path[3]).toEqual(["L", colX(5, m), m.rowH]);
  });
});

describe("laneVerticalRange", () => {
  it("covers the whole lane when it starts and ends at its own commits", () => {
    expect(laneVerticalRange(lane({}))).toEqual({ from: 10, to: 20 });
  });

  it("leaves the connector spans to the connectors", () => {
    expect(laneVerticalRange(lane({ forked: true, joins: true }))).toEqual({ from: 11, to: 19 });
  });

  it("has no vertical part when the lane forks and joins in the same span", () => {
    expect(
      laneVerticalRange(lane({ forked: true, joins: true, firstRow: 4, lastRow: 5 })),
    ).toBeNull();
  });
});

describe("viewport helpers", () => {
  it("finds the visible rows, clamped to the graph", () => {
    expect(visibleRows(0, 280, 1000, m)).toEqual({ first: 0, last: 10 });
    expect(visibleRows(28 * 50 + 5, 100, 1000, m)).toEqual({ first: 50, last: 53 });
    expect(visibleRows(1e9, 100, 40, m)).toEqual({ first: 39, last: 39 });
    expect(visibleRows(0, 100, 0, m)).toBeNull();
  });

  it("maps a vertical position to a row", () => {
    expect(rowAtY(0, 10, m)).toBe(0);
    expect(rowAtY(28 * 3 + 27, 10, m)).toBe(3);
    expect(rowAtY(-1, 10, m)).toBeNull();
    expect(rowAtY(28 * 10, 10, m)).toBeNull();
  });

  it("hits stations with a small margin", () => {
    const x = colX(2, m);
    const y = rowCenterY(7, m);
    expect(isOnStation(x, y, 7, 2, m)).toBe(true);
    expect(isOnStation(x + m.stationR + 3, y, 7, 2, m)).toBe(true);
    expect(isOnStation(x + m.stationR + 6, y, 7, 2, m)).toBe(false);
  });

  it("sizes the graph area from the column count", () => {
    expect(graphContentWidth(0, m)).toBe(28);
    expect(graphContentWidth(1, m)).toBe(28);
    expect(graphContentWidth(4, m)).toBe(28 + 3 * 18);
  });
});

describe("laneAt", () => {
  const lanes = [
    lane({ id: 0, col: 0, firstRow: 0, lastRow: 40 }),
    lane({ id: 1, col: 2, firstRow: 5, lastRow: 30, forked: true, joins: true }),
  ];

  it("finds the lane under a point on its line", () => {
    expect(laneAt(colX(0, m), rowCenterY(10, m), lanes, m)?.id).toBe(0);
    expect(laneAt(colX(2, m) + 3, rowCenterY(12, m), lanes, m)?.id).toBe(1);
  });

  it("misses between lanes, outside the vertical range and beside the line", () => {
    expect(laneAt(colX(1, m), rowCenterY(10, m), lanes, m)).toBeNull();
    expect(laneAt(colX(0, m), rowCenterY(41, m), lanes, m)).toBeNull();
    // Lane 1 forks at row 5 and joins at row 30: its line only spans rows 6 to 29.
    expect(laneAt(colX(2, m), rowCenterY(5, m), lanes, m)).toBeNull();
    expect(laneAt(colX(2, m), rowCenterY(30, m), lanes, m)).toBeNull();
    expect(laneAt(colX(0, m) + 8, rowCenterY(10, m), lanes, m)).toBeNull();
  });
});
