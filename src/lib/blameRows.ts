import type { Blame, BlameCommit } from "./api";

/** One line of the blame view: the first line of a run by the same commit carries its details. */
export interface BlameRow {
  /** 1-based line number. */
  line: number;
  text: string;
  commit: BlameCommit;
  /** Starts a run of lines of the same commit. */
  first: boolean;
  /** How many lines the run has (set on the first line of the run). */
  span: number;
}

/** The lines of a blame with the runs of the same commit marked, so each is described once. */
export function blameRows(blame: Blame): BlameRow[] {
  const rows: BlameRow[] = [];
  let runStart = -1;
  blame.lines.forEach((entry, index) => {
    const first = index === 0 || blame.lines[index - 1].commit !== entry.commit;
    if (first) runStart = rows.length;
    const commit = blame.commits[entry.commit];
    rows.push({ line: index + 1, text: entry.text, commit, first, span: 0 });
    rows[runStart].span += 1;
  });
  return rows;
}

const UNITS: Array<[Intl.RelativeTimeFormatUnit, number]> = [
  ["year", 365 * 24 * 3600],
  ["month", 30 * 24 * 3600],
  ["week", 7 * 24 * 3600],
  ["day", 24 * 3600],
  ["hour", 3600],
  ["minute", 60],
];

/** "3 days ago" for a time in seconds since the Unix epoch; `now` is in milliseconds. */
export function relativeTime(seconds: number, now: number = Date.now()): string {
  const format = new Intl.RelativeTimeFormat("en", { numeric: "auto" });
  const diff = Math.round(seconds - now / 1000);
  for (const [unit, size] of UNITS) {
    if (Math.abs(diff) >= size) return format.format(Math.round(diff / size), unit);
  }
  return format.format(0, "second");
}

/** Whether a CSS color (`rgb(r, g, b)` or `#rrggbb`) is dark; anything else counts as light. */
export function isDarkCssColor(color: string): boolean {
  let r: number;
  let g: number;
  let b: number;
  const rgb = /^rgba?\(\s*(\d+)[\s,]+(\d+)[\s,]+(\d+)/i.exec(color);
  if (rgb) {
    [r, g, b] = [Number(rgb[1]), Number(rgb[2]), Number(rgb[3])];
  } else if (/^#[0-9a-f]{6}/i.test(color)) {
    const value = Number.parseInt(color.slice(1, 7), 16);
    [r, g, b] = [(value >> 16) & 255, (value >> 8) & 255, value & 255];
  } else {
    return false;
  }
  return (0.299 * r + 0.587 * g + 0.114 * b) / 255 < 0.5;
}
