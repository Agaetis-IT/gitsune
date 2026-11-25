export type Density = "compact" | "detailed";

/** Sizes in CSS pixels. */
export interface Metrics {
  rowH: number;
  colW: number;
  lineW: number;
  stationR: number;
  /** Radius of the rounded corners of connectors. */
  cornerR: number;
  /** Space left and right of the lane columns. */
  graphPad: number;
}

export const METRICS: Record<Density, Metrics> = {
  compact: { rowH: 28, colW: 18, lineW: 3.5, stationR: 5, cornerR: 9, graphPad: 14 },
  detailed: { rowH: 46, colW: 24, lineW: 5, stationR: 7, cornerR: 12, graphPad: 16 },
};

export const colX = (col: number, m: Metrics): number => m.graphPad + col * m.colW;

export const rowTop = (row: number, m: Metrics): number => row * m.rowH;

export const rowCenterY = (row: number, m: Metrics): number => row * m.rowH + m.rowH / 2;

/** Width needed to draw `maxCols` columns. */
export function graphContentWidth(maxCols: number, m: Metrics): number {
  return m.graphPad * 2 + Math.max(0, maxCols - 1) * m.colW;
}
