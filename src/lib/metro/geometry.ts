import type { Edge, Lane } from "@/lib/api";
import { colX, type Metrics, rowCenterY } from "./metrics";

export type PathCommand =
  ["M", number, number] | ["L", number, number] | ["Q", number, number, number, number];

/**
 * Orthogonal path with a rounded corner for a connector between a row and the next one.
 * `y` is the center of the row where the span starts. The span is one row high.
 *
 * - `start`: leaves the commit horizontally, turns down, and ends on the next row at the
 *   column of the new lane.
 * - `end`: runs down the lane, turns toward the target column right before the next row,
 *   and ends on the commit there.
 */
export function connectorPath(edge: Edge, y: number, m: Metrics): PathCommand[] {
  const x0 = colX(edge.fromCol, m);
  const x1 = colX(edge.toCol, m);
  const dir = Math.sign(x1 - x0) || 1;
  const r = Math.min(m.cornerR, Math.abs(x1 - x0), m.rowH);
  const next = y + m.rowH;
  if (edge.bend === "start") {
    return [
      ["M", x0, y],
      ["L", x1 - dir * r, y],
      ["Q", x1, y, x1, y + r],
      ["L", x1, next],
    ];
  }
  return [
    ["M", x0, y],
    ["L", x0, next - r],
    ["Q", x0, next, x0 + dir * r, next],
    ["L", x1, next],
  ];
}

/**
 * Rows (inclusive) between which a lane is a straight vertical line, or `null` if it has none.
 * The span where a lane forks away or joins another lane is drawn by its connector instead.
 */
export function laneVerticalRange(lane: Lane): { from: number; to: number } | null {
  const from = lane.forked ? lane.firstRow + 1 : lane.firstRow;
  const to = lane.joins ? lane.lastRow - 1 : lane.lastRow;
  return to >= from ? { from, to } : null;
}

/** First and last row intersecting the viewport (inclusive), `null` for an empty graph. */
export function visibleRows(
  scrollTop: number,
  viewportH: number,
  total: number,
  m: Metrics,
): { first: number; last: number } | null {
  if (total === 0) return null;
  const first = Math.min(total - 1, Math.max(0, Math.floor(scrollTop / m.rowH)));
  const last = Math.min(total - 1, Math.max(first, Math.floor((scrollTop + viewportH) / m.rowH)));
  return { first, last };
}

/** Row under a vertical position in content coordinates, `null` outside the graph. */
export function rowAtY(y: number, total: number, m: Metrics): number | null {
  if (y < 0) return null;
  const row = Math.floor(y / m.rowH);
  return row < total ? row : null;
}

/** Whether a point is on a station (a small margin makes it easier to hit). */
export function isOnStation(px: number, py: number, row: number, col: number, m: Metrics): boolean {
  const dx = px - colX(col, m);
  const dy = py - rowCenterY(row, m);
  const reach = m.stationR + 4;
  return dx * dx + dy * dy <= reach * reach;
}
