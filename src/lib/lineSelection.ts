import type { DiffHunk } from "./api";

/** Indices, among the lines of a hunk, of those that are changes (added or removed). */
export const changeIndices = (hunk: DiffHunk): number[] =>
  hunk.lines.flatMap((line, index) => (line.kind === "context" ? [] : [index]));

/**
 * Whether single lines of a hunk can be picked: not when a line has no line ending, which a
 * partial patch cannot express safely (the whole hunk can still be staged).
 */
export const canPickLines = (hunk: DiffHunk): boolean => hunk.lines.every((line) => !line.noEol);

/** A selection with one line more or less. */
export function toggled(selection: ReadonlySet<number>, index: number): Set<number> {
  const next = new Set(selection);
  if (!next.delete(index)) next.add(index);
  return next;
}

/** Every change of the hunk, or none when they are all selected already. */
export function toggledAll(hunk: DiffHunk, selection: ReadonlySet<number>): Set<number> {
  const all = changeIndices(hunk);
  return all.every((index) => selection.has(index)) ? new Set() : new Set(all);
}

/** The selected indices in order, for the command. */
export const sortedSelection = (selection: ReadonlySet<number>): number[] =>
  [...selection].sort((a, b) => a - b);
