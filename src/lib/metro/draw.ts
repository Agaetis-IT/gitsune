import type { GraphMeta, GraphRow, RefInfo } from "@/lib/api";
import { connectorPath, laneVerticalRange, visibleRows } from "./geometry";
import type { PrBadge } from "../prStatus";
import { drawIcon, type IconName } from "./icons";
import { colX, type Density, type Metrics, rowCenterY, rowTop } from "./metrics";
import { contrastText, laneColor, type Palette } from "./palette";
import { initialsOf, isDarkBackground, personColor, personKey } from "./people";
import { groupRefs, type RefGroup } from "./refGroups";

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
  /** Left edge of the lane area: the column of branch labels is to its left. */
  graphX: number;
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
  /** The author whose commits stay in full while the others fade (`personKey`), or `null`. */
  focusAuthor?: string | null;
  /** Rows matching the current search, tinted. */
  matchRows: ReadonlySet<number>;
  /** Height of the pinned working tree row at the top; the graph scrolls below it. */
  topInset: number;
  /** The working tree row (pinned at the top), `null` for none. */
  wip: WipRow | null;
  /** Draw the initials of the author in the stations, in a color per person. */
  initials: boolean;
  /** Columns shown to the right of the commit message. */
  columns: { author: boolean; date: boolean; sha: boolean };
  /** The icon of a remote, by name. */
  remoteIcon: (remote: string) => IconName;
  /** The pull request badge of each branch, by branch name. */
  prBadges?: ReadonlyMap<string, PrBadge>;
  /** Receives the position of every ref label drawn (for right-click hit testing). */
  pills?: PillRect[];
}

/** Whether a commit is in full: on the highlighted lane and by the focused author. */
function inFocus(input: DrawInput, data: GraphRow): boolean {
  if (input.highlightLane !== null && data.lane !== input.highlightLane) return false;
  return !input.focusAuthor || personKey(data.author) === input.focusAuthor;
}

/** Where a ref label was drawn (screen position), so it can be clicked. */
export interface PillRect {
  x: number;
  y: number;
  w: number;
  h: number;
  ref: RefInfo;
  row: number;
  /** For the pull request badge of a label: the pull request it opens when clicked. */
  badge?: { id: number; title: string };
  /** For the `+N` label: the labels it stands for, to list them when it is clicked. */
  more?: Array<{ label: string; ref: RefInfo }>;
}

/** What a label says about where a branch is: "this computer, origin", "tag". */
export function describeGroup(group: RefGroup): string {
  const places: string[] = [];
  if (group.stash) return `${group.name} (stash)`;
  if (group.tag) places.push("tag");
  if (group.local) places.push("this computer");
  for (const { remote } of group.remotes) if (!places.includes(remote)) places.push(remote);
  return `${group.name} (${places.join(", ")})`;
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
/** Lines while an author is in focus. */
const FOCUS_LANE = 0.45;
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
    drawRefs(inner, range);
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
  ctx.rect(input.graphX, 0, input.graphViewportW, height);
  ctx.clip();
  const x = input.graphX + colX(wip.col, m) - input.graphScrollX;
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
  // Every row is tinted in the color of its branch, faintly, to follow a line across the page.
  for (let row = range.first; row <= range.last; row++) {
    const data = input.getRow(row);
    const lane = data ? input.meta.lanes[data.lane] : undefined;
    if (!lane) continue;
    const faded = data !== undefined && !inFocus(input, data);
    ctx.globalAlpha = faded ? 0.02 : 0.07;
    ctx.fillStyle = laneColor(theme.palette, lane.color);
    ctx.fillRect(0, rowTop(row, m) - scrollTop, width, m.rowH);
  }
  ctx.globalAlpha = 1;
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
  ctx.rect(input.graphX, 0, input.graphViewportW, input.height);
  ctx.clip();
  ctx.translate(input.graphX - input.graphScrollX, -input.scrollTop);
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
  // With an author in focus the lines are pale: only their commits tell where that author worked.
  const lineAlpha = input.focusAuthor ? FOCUS_LANE : 1;
  const alphaOf = (laneId: number) =>
    highlight === null || laneId === highlight ? lineAlpha : FADED_LANE;

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
    ctx.globalAlpha = inFocus(input, data) ? 1 : FADED_LANE;
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
  if (isMerge) {
    // A merge is a plain dot in the color of its branch: no initials, no ring.
    ctx.fillStyle = color;
    ctx.beginPath();
    ctx.arc(x, y, Math.max(3.5, m.stationR * (input.initials ? 0.45 : 0.6)), 0, Math.PI * 2);
    ctx.fill();
    return;
  }
  if (input.initials) {
    drawInitialsStation(ctx, x, y, color, false, input, author);
    return;
  }
  ctx.lineWidth = m.lineW * 0.7;
  ctx.fillStyle = theme.background;
  ctx.strokeStyle = color;
  ctx.beginPath();
  ctx.arc(x, y, m.stationR, 0, Math.PI * 2);
  ctx.fill();
  ctx.stroke();
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
  const { ctx, width, theme, metrics: m, density, columns } = input;
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
    ctx.globalAlpha = inFocus(input, data) ? 1 : FADED_TEXT;
    const summaryY = detailed ? centerY - 8 : centerY;

    // The optional columns, right-aligned: author, date, id (the id is the rightmost).
    let right = width - 12;
    if (available > 520) {
      ctx.font = `11px ${FONT}`;
      ctx.fillStyle = theme.muted;
      ctx.textAlign = "right";
      if (columns.sha) {
        ctx.fillText(data.id.slice(0, 7), right, centerY);
        right -= 64;
      }
      if (columns.date) {
        ctx.fillText(dateFormat.format(new Date(data.time * 1000)), right, centerY);
        right -= 92;
      }
      if (columns.author) {
        ctx.fillText(fitText(ctx, data.author, 130), right, centerY);
        right -= 142;
      }
      ctx.textAlign = "left";
    }

    ctx.textAlign = "left";
    ctx.font = `${detailed ? 13 : 12}px ${FONT}`;
    ctx.fillStyle = theme.foreground;
    ctx.fillText(fitText(ctx, data.summary, right - input.textX), input.textX, summaryY);

    if (detailed) {
      // Under the message: what is not already in a column.
      const parts: string[] = [];
      if (!columns.author || available <= 520) parts.push(data.author);
      if (!columns.date || available <= 520) {
        parts.push(dateFormat.format(new Date(data.time * 1000)));
      }
      if (!columns.sha || available <= 520) parts.push(data.id.slice(0, 7));
      if (parts.length > 0) {
        ctx.font = `11px ${FONT}`;
        ctx.fillStyle = theme.muted;
        ctx.fillText(fitText(ctx, parts.join(" · "), available), input.textX, centerY + 11);
      }
    }
  }
  ctx.restore();
}

/**
 * The column of branch and tag labels, each linked to its station by a line in the color of the
 * branch. Several refs on one commit are shown as far as they fit, then as `+N`.
 */
function drawRefs(input: DrawInput, range: Range): void {
  const { ctx, theme, metrics: m } = input;
  const columnW = input.graphX;
  if (columnW < 60) return;

  ctx.save();
  ctx.beginPath();
  ctx.rect(0, 0, input.graphX + input.graphViewportW, input.height);
  ctx.clip();
  ctx.textBaseline = "middle";
  ctx.textAlign = "left";

  for (let row = range.first; row <= range.last; row++) {
    const data = input.getRow(row);
    if (!data || data.refs.length === 0) continue;
    const lane = input.meta.lanes[data.lane];
    const color = laneColor(theme.palette, lane?.color ?? null);
    const y = rowCenterY(row, m) - input.scrollTop;
    const groups = groupRefs(data.refs);
    if (groups.length === 0) continue;
    ctx.globalAlpha = inFocus(input, data) ? 1 : FADED_TEXT;

    const height = Math.min(24, Math.round(m.rowH * 0.64));
    const left = 8;
    const room = columnW - left - 8;
    let x = left;
    let shown = 0;
    for (const group of groups) {
      const hidden = groups.length - shown;
      const reserve = hidden > 1 ? 34 : 0;
      const label = pillLayout(ctx, group, room - (x - left) - reserve, input);
      if (shown > 0 && label.width > room - (x - left) - reserve) break;
      drawPill(ctx, group, label, x, y, height, color, input);
      if (label.badge) {
        // Before the label itself, so a click on the badge finds it first.
        input.pills?.push({
          x: x + label.width - 4 - BADGE_W,
          y: y - height / 2 + input.topInset,
          w: BADGE_W,
          h: height,
          ref: group.primary,
          row,
          badge: { id: label.badge.id, title: label.badge.title },
        });
      }
      input.pills?.push({
        x,
        y: y - height / 2 + input.topInset,
        w: label.width,
        h: height,
        ref: group.primary,
        row,
      });
      x += label.width + 4;
      shown++;
    }
    if (shown < groups.length) {
      const text = `+${groups.length - shown}`;
      ctx.font = `600 11px ${FONT}`;
      const w = ctx.measureText(text).width + 12;
      ctx.beginPath();
      ctx.roundRect(x, y - height / 2, w, height, height / 2);
      ctx.fillStyle = theme.muted;
      ctx.globalAlpha *= 0.25;
      ctx.fill();
      ctx.globalAlpha = inFocus(input, data) ? 1 : FADED_TEXT;
      ctx.fillStyle = theme.foreground;
      ctx.fillText(text, x + 6, y + 0.5);
      input.pills?.push({
        x,
        y: y - height / 2 + input.topInset,
        w,
        h: height,
        ref: groups[shown].primary,
        row,
        more: groups.slice(shown).map((g) => ({ label: describeGroup(g), ref: g.primary })),
      });
      x += w + 4;
    }

    // The line from the labels to the station (under the lanes, which are drawn after).
    const stationX = input.graphX + colX(data.col, m) - input.graphScrollX;
    const toX = Math.max(input.graphX, stationX);
    if (toX > x) {
      ctx.strokeStyle = color;
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.moveTo(x - 2, y);
      ctx.lineTo(toX, y);
      ctx.stroke();
    }
  }
  ctx.globalAlpha = 1;
  ctx.restore();
}

const ICON = 12;
const ICON_STEP = 15;
/** Room of the pull request badge at the end of a label. */
const BADGE_W = 16;

interface PillLayout {
  text: string;
  width: number;
  icons: IconName[];
  badge: PrBadge | null;
}

/** What a label shows and how wide it is, with its name shortened to fit `maxWidth`. */
function pillLayout(
  ctx: CanvasRenderingContext2D,
  group: RefGroup,
  maxWidth: number,
  input: DrawInput,
): PillLayout {
  const icons: IconName[] = [];
  if (group.stash) icons.push("archive");
  if (group.tag) icons.push("tag");
  if (group.local) icons.push("laptop");
  const seen = new Set<IconName>();
  for (const { remote } of group.remotes) {
    const icon = input.remoteIcon(remote);
    if (!seen.has(icon)) {
      seen.add(icon);
      icons.push(icon);
    }
  }
  const check = group.isHead ? ICON_STEP : 0;
  const badge = group.tag || group.stash ? null : (input.prBadges?.get(group.name) ?? null);
  const fixed =
    7 + check + (icons.length > 0 ? 4 + icons.length * ICON_STEP : 0) + (badge ? BADGE_W : 0) + 4;
  ctx.font = `${group.isHead ? "700" : "600"} 11px ${FONT}`;
  const text = fitText(ctx, group.name, Math.max(30, maxWidth - fixed));
  return { text, width: fixed + ctx.measureText(text).width, icons, badge };
}

function drawPill(
  ctx: CanvasRenderingContext2D,
  group: RefGroup,
  layout: PillLayout,
  x: number,
  y: number,
  height: number,
  color: string,
  input: DrawInput,
): void {
  const { theme } = input;
  const top = y - height / 2;
  ctx.beginPath();
  ctx.roundRect(x, top, layout.width, height, height / 2);
  let ink: string;
  if (group.local) {
    ctx.fillStyle = color;
    ctx.fill();
    if (group.isHead) {
      ctx.lineWidth = 2;
      ctx.strokeStyle = theme.foreground;
      ctx.stroke();
    }
    ink = contrastText(color);
  } else if (group.tag || group.stash) {
    ctx.lineWidth = 1;
    ctx.strokeStyle = theme.muted;
    ctx.stroke();
    ink = theme.muted;
  } else {
    ctx.lineWidth = 1.5;
    ctx.strokeStyle = color;
    ctx.stroke();
    ink = color;
  }
  let cursor = x + 7;
  if (group.isHead) {
    drawIcon(ctx, "check", cursor + ICON / 2, y, ICON, ink);
    cursor += ICON_STEP;
  }
  ctx.font = `${group.isHead ? "700" : "600"} 11px ${FONT}`;
  ctx.fillStyle = ink;
  ctx.textAlign = "left";
  ctx.fillText(layout.text, cursor, y + 0.5);
  cursor += ctx.measureText(layout.text).width + 4;
  for (const icon of layout.icons) {
    drawIcon(ctx, icon, cursor + ICON / 2, y, ICON, ink);
    cursor += ICON_STEP;
  }
  if (layout.badge) {
    // A round plate keeps the colored icon readable on any label.
    const cx = x + layout.width - 4 - BADGE_W / 2;
    ctx.beginPath();
    ctx.arc(cx, y, 7, 0, Math.PI * 2);
    ctx.fillStyle = theme.background;
    ctx.fill();
    drawIcon(ctx, layout.badge.icon, cx, y, 10, layout.badge.color);
  }
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
