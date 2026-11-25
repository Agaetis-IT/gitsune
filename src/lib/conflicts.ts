/**
 * The merge logic of the conflict resolver: it reads the conflict markers git wrote in a file
 * and builds the resolved text from one choice per block. Nothing here touches the disk.
 */

export type Segment =
  | { type: "text"; text: string }
  | {
      type: "conflict";
      /** Position among the conflicts of the file. */
      id: number;
      ours: string;
      /** The common ancestor, when git was asked for it (`merge.conflictStyle=diff3`). */
      base: string | null;
      theirs: string;
      /** What followed the markers (`HEAD`, a branch name), to label the sides. */
      oursLabel: string;
      theirsLabel: string;
    };

/** What to put in place of a conflict block; `null` leaves it unresolved. */
export type Choice = "ours" | "theirs" | "both" | "bothReversed" | null;

const START = /^<{7}(?: (.*?))?\r?\n?$/;
const BASE = /^\|{7}(?: .*)?\r?\n?$/;
const MIDDLE = /^={7}\r?\n?$/;
const END = /^>{7}(?: (.*?))?\r?\n?$/;

/** Cuts a text into lines, each keeping its own line ending. */
const linesOf = (text: string): string[] => text.match(/[^\n]*\n|[^\n]+$/g) ?? [];

/**
 * Splits a file into plain text and conflict blocks. A block that is not closed is left as
 * text, so a stray marker never loses content.
 */
export function parseConflicts(text: string): Segment[] {
  const lines = linesOf(text);
  const segments: Segment[] = [];
  let plain: string[] = [];
  let id = 0;
  const flush = () => {
    if (plain.length > 0) segments.push({ type: "text", text: plain.join("") });
    plain = [];
  };

  for (let i = 0; i < lines.length; i++) {
    const start = START.exec(lines[i]);
    if (!start) {
      plain.push(lines[i]);
      continue;
    }
    const ours: string[] = [];
    const base: string[] = [];
    const theirs: string[] = [];
    let part: "ours" | "base" | "theirs" = "ours";
    let hasBase = false;
    let theirsLabel = "";
    let closed = false;
    let j = i + 1;
    for (; j < lines.length; j++) {
      const line = lines[j];
      if (part === "ours" && BASE.test(line)) {
        part = "base";
        hasBase = true;
      } else if (part !== "theirs" && MIDDLE.test(line)) {
        part = "theirs";
      } else if (part === "theirs" && END.test(line)) {
        theirsLabel = END.exec(line)?.[1] ?? "";
        closed = true;
        break;
      } else {
        (part === "ours" ? ours : part === "base" ? base : theirs).push(line);
      }
    }
    if (!closed) {
      plain.push(lines[i]);
      continue;
    }
    flush();
    segments.push({
      type: "conflict",
      id: id++,
      ours: ours.join(""),
      base: hasBase ? base.join("") : null,
      theirs: theirs.join(""),
      oursLabel: start[1] ?? "",
      theirsLabel,
    });
    i = j;
  }
  flush();
  return segments;
}

export const conflictsOf = (segments: Segment[]) =>
  segments.filter((s): s is Extract<Segment, { type: "conflict" }> => s.type === "conflict");

/** A line break is added between two sides that are put one after the other. */
const endWithNewline = (text: string) => (text === "" || text.endsWith("\n") ? text : `${text}\n`);

/** The text of one block for a choice. `null` gives the block back with its markers. */
export function resolveBlock(
  block: Extract<Segment, { type: "conflict" }>,
  choice: Choice,
): string {
  switch (choice) {
    case "ours":
      return block.ours;
    case "theirs":
      return block.theirs;
    case "both":
      return endWithNewline(block.ours) + block.theirs;
    case "bothReversed":
      return endWithNewline(block.theirs) + block.ours;
    default: {
      const base = block.base === null ? "" : `||||||| base\n${endWithNewline(block.base)}`;
      return (
        `<<<<<<< ${block.oursLabel || "ours"}\n${endWithNewline(block.ours)}${base}` +
        `=======\n${endWithNewline(block.theirs)}>>>>>>> ${block.theirsLabel || "theirs"}\n`
      );
    }
  }
}

/** The file with each block replaced by its choice (blocks without one keep their markers). */
export function compose(segments: Segment[], choices: Record<number, Choice>): string {
  return segments
    .map((s) => (s.type === "text" ? s.text : resolveBlock(s, choices[s.id] ?? null)))
    .join("");
}

export function unresolvedCount(segments: Segment[], choices: Record<number, Choice>): number {
  return conflictsOf(segments).filter((block) => (choices[block.id] ?? null) === null).length;
}

/** Whether text still holds a complete conflict block. */
export const hasMarkers = (text: string): boolean => conflictsOf(parseConflicts(text)).length > 0;

/**
 * The line of the result (counted from 0) where a block starts, so the result can be scrolled to
 * the conflict being looked at. `null` for a block the file does not have.
 */
export function resultLineOf(
  segments: Segment[],
  choices: Record<number, Choice>,
  blockId: number,
): number | null {
  const at = segments.findIndex((s) => s.type === "conflict" && s.id === blockId);
  if (at === -1) return null;
  const before = compose(segments.slice(0, at), choices);
  return before === "" ? 0 : before.split("\n").length - 1;
}

/** The text of a whole side of the file: plain text plus that side of every block. */
export function sideText(segments: Segment[], side: "ours" | "theirs"): string {
  return segments.map((s) => (s.type === "text" ? s.text : s[side])).join("");
}
