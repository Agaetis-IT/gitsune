import type { GraphMeta, GraphRow, RefInfo } from "@/lib/api";
import { connectorPath, laneVerticalRange, visibleRows } from "./geometry";
import { colX, type Density, type Metrics, rowCenterY, rowTop } from "./metrics";
import { contrastText, laneColor, type Palette } from "./palette";
import { initialsOf, isDarkBackground, personColor } from "./people";

export interface Theme {
  background: string;
  foreground: string;
  muted: string;
  palette: Palette;
}

export interface DrawInput {
  ctx: CanvasRenderingContext2D;
  /** Canvas size in CSS pixels. */
  width: number;
  height: number;
  dpr: number;
  scrollTop: number;
  /** Horizontal offset of the lane area (it scrolls on its own, the text does not). */
  graphScrollX: number;
  graphViewportW: number;
  textX: number;
  meta: GraphMeta;
  metrics: Metrics;
  density: Density;
  theme: Theme;
  getRow: (row: number) => GraphRow | undefined;
  selectedRow: number | null;
  hoverRow: number | null;
  /** The commit HEAD points at: its row is tinted so the current position is always visible. */
  headRow: number | null;

  /** Lane drawn in full while the others fade; `null` for no highlight. */
  highlightLane: number | null;
  /** Rows matching the current search, tinted. */
  matchRows: ReadonlySet<number>;
  /** Height of the pinned working tree row at the top; the graph scrolls below it. */
  topInset: number;
  /** The working tree row (pinned at the top), `null` for none. */
  wip: WipRow | null;
  /** Draw the initials of the author in the stations, in a color per person. */
  initials: boolean;
  /** Receives the position of every ref label drawn (for right-click hit testing). */
  pills?: PillRect[];
}

/** Where a ref label was drawn (screen position), so it can be clicked. */
export interface PillRect {
  x: number;
  y: number;
  w: number;
  h: number;
  ref: RefInfo;
  row: number;
}

/** The pinned row that stands for the uncommitted changes. */
export interface WipRow {
  /** Column of the lane of HEAD, where the dashed station sits. */
  col: number;
  /** Number of changed files. */
  count: number;
  selected: boolean;
}

/** Opacity of what is not highlighted. */
const FADED_LANE = 0.18;
const FADED_TEXT = 0.35;
const MATCH_TINT = "#facc15";

const FONT = "system-ui, -apple-system, 'Segoe UI', sans-serif";
const dateFormat = new Intl.DateTimeFormat(undefined, {
  year: "numeric",
  month: "short",
  day: "numeric",
});

/** Draws the visible part of the graph: lanes, connectors, stations, and the commit text. */
export function draw(input: DrawInput): void {
  const { ctx, width, height, dpr, theme } = input;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, width, height);
  ctx.fillStyle = theme.background;
  ctx.fillRect(0, 0, width, height);

  // The graph is drawn below the pinned working tree row.
  const inner = input.topInset > 0 ? { ...input, height: height - input.topInset } : input;
  const range = visibleRows(inner.scrollTop, inner.height, input.meta.total, input.metrics);
  if (range) {
    ctx.save();
    if (input.topInset > 0) {
      ctx.beginPath();
      ctx.rect(0, input.topInset, width, height - input.topInset);
      ctx.clip();
      ctx.translate(0, input.topInset);
    }
    drawBands(inner, range);
    drawLanes(inner, range);
    drawText(inner, range);
    ctx.restore();
  }
  if (input.wip) drawWipRow(input, input.wip);
}

/** The working tree row: a dashed station on the lane of HEAD, and a label. */
function drawWipRow(input: DrawInput, wip: WipRow): void {
  const { ctx, width, theme, metrics: m } = input;
  const height = input.topInset;

  // Row background (stronger when selected), with a line under it.
  ctx.fillStyle = theme.background;
  ctx.fillRect(0, 0, width, height);
  if (wip.selected) {
    ctx.globalAlpha = 0.1;
    ctx.fillStyle = theme.foreground;
    ctx.fillRect(0, 0, width, height);
    ctx.globalAlpha = 1;
    ctx.fillStyle = theme.foreground;
    ctx.fillRect(0, 0, 3, height);
  }
  ctx.globalAlpha = 0.25;
  ctx.fillStyle = theme.muted;
  ctx.fillRect(0, height - 1, width, 1);
  ctx.globalAlpha = 1;

  // Station and the dashed line going down into the graph.
  ctx.save();
  ctx.beginPath();
  ctx.rect(0, 0, input.graphViewportW, height);
  ctx.clip();
  const x = colX(wip.col, m) - input.graphScrollX;
  const y = height / 2;
  ctx.strokeStyle = theme.foreground;
  ctx.fillStyle = theme.background;
  ctx.lineWidth = m.lineW * 0.7;
  ctx.setLineDash([3, 3]);
  ctx.beginPath();
  ctx.moveTo(x, y + m.stationR);
  ctx.lineTo(x, height);
  ctx.stroke();
  ctx.beginPath();
  ctx.arc(x, y, m.stationR, 0, Math.PI * 2);
  ctx.fill();
  ctx.stroke();
  ctx.restore();

  // Label.
  ctx.textBaseline = "middle";
  ctx.textAlign = "left";
  ctx.font = `600 12px ${FONT}`;
  ctx.fillStyle = theme.foreground;
  ctx.fillText("Working tree", input.textX, y);
  const label = wip.count === 0 ? "no changes" : `${wip.count} change${wip.count === 1 ? "" : "s"}`;
  ctx.font = `11px ${FONT}`;
  ctx.fillStyle = theme.muted;
  ctx.fillText(label, input.textX + 92, y);
}

type Range = { first: number; last: number };

function drawBands(input: DrawInput, range: Range): void {
  const { ctx, width, theme, metrics: m, scrollTop } = input;
  const band = (row: number | null, alpha: number) => {
    if (row === null || row < range.first || row > range.last) return;
    ctx.globalAlpha = alpha;
    ctx.fillStyle = theme.foreground;
    ctx.fillRect(0, rowTop(row, m) - scrollTop, width, m.rowH);
    ctx.globalAlpha = 1;
  };
  for (let row = range.first; row <= range.last; row++) {
    if (!input.matchRows.has(row)) continue;
    ctx.globalAlpha = 0.16;
    ctx.fillStyle = MATCH_TINT;
    ctx.fillRect(0, rowTop(row, m) - scrollTop, width, m.rowH);
    ctx.globalAlpha = 1;
  }
  band(input.hoverRow, 0.05);
  band(input.selectedRow, 0.1);

  // Where the user is: a tinted row with a bar in the color of its branch.
  if (input.headRow !== null && input.headRow >= range.first && input.headRow <= range.last) {
    const row = input.getRow(input.headRow);
    const lane = row ? input.meta.lanes[row.lane] : undefined;
    const color = lane ? laneColor(theme.palette, lane.color) : theme.foreground;
    const y = rowTop(input.headRow, m) - scrollTop;
    ctx.globalAlpha = 0.14;
    ctx.fillStyle = color;
    ctx.fillRect(0, y, width, m.rowH);
    ctx.globalAlpha = 1;
    ctx.fillStyle = color;
    ctx.fillRect(0, y, 5, m.rowH);
  }

  if (
    input.selectedRow !== null &&
    input.selectedRow >= range.first &&
    input.selectedRow <= range.last
  ) {
    const row = input.getRow(input.selectedRow);
    const lane = row ? input.meta.lanes[row.lane] : undefined;
    ctx.fillStyle = lane ? laneColor(theme.palette, lane.color) : theme.foreground;
    ctx.fillRect(0, rowTop(input.selectedRow, m) - scrollTop, 3, m.rowH);
  }
}

function drawLanes(input: DrawInput, range: Range): void {
  const { ctx, meta, theme, metrics: m } = input;
  ctx.save();
  ctx.beginPath();
  ctx.rect(0, 0, input.graphViewportW, input.height);
  ctx.clip();
  ctx.translate(-input.graphScrollX, -input.scrollTop);
  ctx.lineWidth = m.lineW;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";

  const minX = input.graphScrollX - m.colW;
  const maxX = input.graphScrollX + input.graphViewportW + m.colW;

  const highlight = input.highlightLane;
  // With a highlight, faded lanes are drawn first so the highlighted one is on top.
  const passes = highlight === null ? [true] : [false, true];
  const inPass = (laneId: number, focusedPass: boolean) =>
    highlight === null || (laneId === highlight) === focusedPass;
  const alphaOf = (laneId: number) => (highlight === null || laneId === highlight ? 1 : FADED_LANE);

  for (const focusedPass of passes) {
    // Straight vertical parts of the lanes.
    for (const lane of meta.lanes) {
      if (!inPass(lane.id, focusedPass)) continue;
      const vertical = laneVerticalRange(lane);
      if (!vertical || vertical.to < range.first - 1 || vertical.from > range.last + 1) continue;
      const x = colX(lane.col, m);
      if (x < minX || x > maxX) continue;
      ctx.globalAlpha = alphaOf(lane.id);
      ctx.strokeStyle = laneColor(theme.palette, lane.color);
      ctx.beginPath();
      ctx.moveTo(x, rowCenterY(Math.max(vertical.from, range.first - 1), m));
      ctx.lineTo(x, rowCenterY(Math.min(vertical.to, range.last + 1), m));
      ctx.stroke();
    }

    // Connectors where lanes fork away or join.
    for (let row = Math.max(0, range.first - 1); row <= range.last; row++) {
      const data = input.getRow(row);
      if (!data) continue;
      for (const edge of data.edges) {
        const lane = meta.lanes[edge.lane];
        if (!lane || !inPass(lane.id, focusedPass)) continue;
        const left = Math.min(colX(edge.fromCol, m), colX(edge.toCol, m));
        const right = Math.max(colX(edge.fromCol, m), colX(edge.toCol, m));
        if (right < minX || left > maxX) continue;
        ctx.globalAlpha = alphaOf(lane.id);
        ctx.strokeStyle = laneColor(theme.palette, lane.color);
        ctx.beginPath();
        for (const command of connectorPath(edge, rowCenterY(row, m), m)) {
          if (command[0] === "M") ctx.moveTo(command[1], command[2]);
          else if (command[0] === "L") ctx.lineTo(command[1], command[2]);
          else ctx.quadraticCurveTo(command[1], command[2], command[3], command[4]);
        }
        ctx.stroke();
      }
    }
  }

  // Stations on top of the lines.
  for (let row = range.first; row <= range.last; row++) {
    const data = input.getRow(row);
    if (!data) continue;
    const x = colX(data.col, m);
    if (x < minX || x > maxX) continue;
    const lane = meta.lanes[data.lane];
    ctx.globalAlpha = alphaOf(data.lane);
    drawStation(
      ctx,
      x,
      rowCenterY(row, m),
      laneColor(theme.palette, lane?.color ?? null),
      data.parents.length > 1,
      input,
      data.author,
    );
  }
  ctx.globalAlpha = 1;
  ctx.restore();
}

function drawStation(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  color: string,
  isMerge: boolean,
  input: DrawInput,
  author: string,
): void {
  const { theme, metrics: m } = input;
  if (input.initials) {
    drawInitialsStation(ctx, x, y, color, isMerge, input, author);
    return;
  }
  ctx.lineWidth = m.lineW * 0.7;
  ctx.fillStyle = theme.background;
  ctx.strokeStyle = color;
  if (isMerge) {
    // Interchange: bigger ring with a dot in the middle.
    ctx.beginPath();
    ctx.arc(x, y, m.stationR + 2, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();
    ctx.fillStyle = theme.foreground;
    ctx.beginPath();
    ctx.arc(x, y, m.stationR * 0.4, 0, Math.PI * 2);
    ctx.fill();
  } else {
    ctx.beginPath();
    ctx.arc(x, y, m.stationR, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();
  }
  ctx.lineWidth = m.lineW;
}

/**
 * A station with the initials of its author on a color of their own; the ring keeps the color
 * of the branch. An interchange (a merge) has a second ring around it.
 */
function drawInitialsStation(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  lane: string,
  isMerge: boolean,
  input: DrawInput,
  author: string,
): void {
  const { theme, metrics: m } = input;
  const person = personColor(author, isDarkBackground(theme.background));
  const radius = m.stationR;
  const ring = Math.max(2, m.lineW * 0.6);

  if (isMerge) {
    ctx.lineWidth = ring * 0.7;
    ctx.strokeStyle = lane;
    ctx.fillStyle = theme.background;
    ctx.beginPath();
    ctx.arc(x, y, radius + ring + 1.5, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();
  }
  ctx.fillStyle = person.fill;
  ctx.strokeStyle = lane;
  ctx.lineWidth = ring;
  ctx.beginPath();
  ctx.arc(x, y, radius - ring / 2, 0, Math.PI * 2);
  ctx.fill();
  ctx.stroke();

  ctx.font = `700 ${Math.round(radius * 0.8)}px ${FONT}`;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillStyle = person.text;
  ctx.fillText(initialsOf(author), x, y + 0.5);
  ctx.textAlign = "left";
  ctx.lineWidth = m.lineW;
}

function drawText(input: DrawInput, range: Range): void {
  const { ctx, width, theme, metrics: m, density } = input;
  const detailed = density === "detailed";
  const available = width - input.textX - 12;
  if (available < 60) return;

  ctx.save();
  ctx.beginPath();
  ctx.rect(input.textX - 4, 0, width - input.textX + 4, input.height);
  ctx.clip();
  ctx.textBaseline = "middle";

  for (let row = range.first; row <= range.last; row++) {
    const data = input.getRow(row);
    const centerY = rowCenterY(row, m) - input.scrollTop;
    if (!data) {
      // Rows still loading: a faint placeholder bar.
      ctx.globalAlpha = 0.08;
      ctx.fillStyle = theme.foreground;
      ctx.fillRect(input.textX, centerY - 4, Math.min(available * 0.5, 240), 8);
      ctx.globalAlpha = 1;
      continue;
    }
    const lane = input.meta.lanes[data.lane];
    const color = laneColor(theme.palette, lane?.color ?? null);
    ctx.globalAlpha =
      input.highlightLane === null || data.lane === input.highlightLane ? 1 : FADED_TEXT;
    const summaryY = detailed ? centerY - 8 : centerY;

    // Right-aligned author and date in compact mode.
    let right = width - 12;
    if (!detailed && available > 520) {
      const date = dateFormat.format(new Date(data.time * 1000));
      ctx.font = `11px ${FONT}`;
      ctx.fillStyle = theme.muted;
      ctx.textAlign = "right";
      ctx.fillText(date, right, centerY);
      right -= 92;
      ctx.fillText(fitText(ctx, data.author, 130), right, centerY);
      right -= 142;
      ctx.textAlign = "left";
    }

    let x = input.textX;
    ctx.textAlign = "left";
    x = drawRefPills(ctx, data.refs, x, summaryY, color, input, right, row);
    ctx.font = `${detailed ? 13 : 12}px ${FONT}`;
    ctx.fillStyle = theme.foreground;
    ctx.fillText(fitText(ctx, data.summary, right - x), x, summaryY);

    if (detailed) {
      ctx.font = `11px ${FONT}`;
      ctx.fillStyle = theme.muted;
      const meta = `${data.author} · ${dateFormat.format(new Date(data.time * 1000))} · ${data.id.slice(0, 7)}`;
      ctx.fillText(fitText(ctx, meta, available), input.textX, centerY + 11);
    }
  }
  ctx.restore();
}

/** Draws ref labels from `x`, returns the x where the next element can start. */
function drawRefPills(
  ctx: CanvasRenderingContext2D,
  refs: RefInfo[],
  startX: number,
  y: number,
  color: string,
  input: DrawInput,
  limitX: number,
  row: number,
): number {
  const { theme, metrics: m } = input;
  const height = Math.round(m.rowH * (input.density === "detailed" ? 0.4 : 0.64));
  const shown = refs.slice(0, 3);
  let x = startX;
  const labels = shown.map((ref) => ({ ref, text: ref.name }));
  if (refs.length > shown.length) {
    labels.push({ ref: refs[0], text: `+${refs.length - shown.length}` });
  }
  for (const { ref, text } of labels) {
    const extra = text.startsWith("+") && ref === refs[0] && refs.length > shown.length;
    ctx.font = `${ref.isHead && !extra ? "700" : "600"} 11px ${FONT}`;
    const label = fitText(ctx, text, Math.min(180, limitX - x - 12));
    const w = ctx.measureText(label).width + 14;
    if (x + w > limitX - 40) break;
    const top = y - height / 2;
    if (!extra) input.pills?.push({ x, y: top + input.topInset, w, h: height, ref, row });
    ctx.beginPath();
    ctx.roundRect(x, top, w, height, height / 2);
    if (extra) {
      ctx.fillStyle = theme.muted;
      ctx.globalAlpha = 0.25;
      ctx.fill();
      ctx.globalAlpha = 1;
      ctx.fillStyle = theme.foreground;
    } else if (ref.kind === "local") {
      ctx.fillStyle = color;
      ctx.fill();
      if (ref.isHead) {
        ctx.lineWidth = 2;
        ctx.strokeStyle = theme.foreground;
        ctx.stroke();
      }
      ctx.fillStyle = contrastText(color);
    } else if (ref.kind === "remote") {
      ctx.lineWidth = 1.5;
      ctx.strokeStyle = color;
      ctx.stroke();
      ctx.fillStyle = color;
    } else {
      ctx.lineWidth = 1;
      ctx.strokeStyle = theme.muted;
      ctx.stroke();
      ctx.fillStyle = theme.muted;
    }
    ctx.textAlign = "left";
    ctx.fillText(label, x + 7, y + 0.5);
    x += w + 6;
  }
  return x + (labels.length > 0 ? 2 : 0);
}

/** Shortens `text` with an ellipsis so it fits in `maxWidth` pixels (current font). */
export function fitText(ctx: CanvasRenderingContext2D, text: string, maxWidth: number): string {
  if (maxWidth <= 0) return "";
  if (ctx.measureText(text).width <= maxWidth) return text;
  let low = 0;
  let high = text.length;
  while (low < high) {
    const mid = Math.ceil((low + high) / 2);
    if (ctx.measureText(`${text.slice(0, mid)}…`).width <= maxWidth) low = mid;
    else high = mid - 1;
  }
  return low === 0 ? "" : `${text.slice(0, low)}…`;
}
