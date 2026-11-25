import type { RebaseCommit, TodoAction, TodoItem } from "@/lib/api";

/** The list the interactive rebase editor shows: one row per commit, in replay order. */
export interface TodoRow extends RebaseCommit {
  action: TodoAction;
  /** New message (reword) or combined message (squash); empty keeps git's own. */
  message: string;
}

export const ACTIONS: Array<{ action: TodoAction; label: string; help: string }> = [
  { action: "pick", label: "Pick", help: "Keep the commit as it is." },
  { action: "reword", label: "Reword", help: "Keep the changes, write a new message." },
  { action: "edit", label: "Edit", help: "Stop after this commit so you can change it." },
  {
    action: "squash",
    label: "Squash",
    help: "Fold into the commit above and combine the messages.",
  },
  { action: "fixup", label: "Fixup", help: "Fold into the commit above and drop this message." },
  { action: "drop", label: "Drop", help: "Remove the commit and its changes." },
];

export const rowsOf = (commits: RebaseCommit[]): TodoRow[] =>
  commits.map((commit) => ({ ...commit, action: "pick", message: "" }));

/** Moves a row by `by` places (negative goes up); out-of-range moves change nothing. */
export function moveRow(rows: TodoRow[], index: number, by: number): TodoRow[] {
  const target = index + by;
  if (index < 0 || index >= rows.length || target < 0 || target >= rows.length) return rows;
  const next = [...rows];
  const [row] = next.splice(index, 1);
  next.splice(target, 0, row);
  return next;
}

/** The first commit that is kept cannot be folded into one above it. */
export function firstKeptIndex(rows: TodoRow[]): number {
  return rows.findIndex((row) => row.action !== "drop");
}

/** What is wrong with the list, if anything. `null` means it can be run. */
export function problem(rows: TodoRow[]): string | null {
  const first = firstKeptIndex(rows);
  if (first !== -1 && ["squash", "fixup"].includes(rows[first].action)) {
    return "The first commit that is kept cannot be squashed or fixed up: there is nothing above it.";
  }
  return null;
}

/** Whether the list changes anything compared with a plain rebase. */
export const isUntouched = (rows: TodoRow[], original: RebaseCommit[]): boolean =>
  rows.length === original.length &&
  rows.every((row, i) => row.id === original[i].id && row.action === "pick" && !row.message.trim());

export const toItems = (rows: TodoRow[]): TodoItem[] =>
  rows.map((row) => ({
    action: row.action,
    id: row.id,
    message: ["reword", "squash"].includes(row.action) && row.message.trim() ? row.message : null,
  }));

export const dropCount = (rows: TodoRow[]): number =>
  rows.filter((row) => row.action === "drop").length;

/**
 * Moves a row to an insertion point between rows (`to` counts the gaps: 0 is above the first row,
 * `rows.length` below the last), as a drop does. Dropping a row where it already is changes nothing.
 */
export function moveRowTo(rows: TodoRow[], from: number, to: number): TodoRow[] {
  if (from < 0 || from >= rows.length || to < 0 || to > rows.length) return rows;
  if (to === from || to === from + 1) return rows;
  const next = [...rows];
  const [row] = next.splice(from, 1);
  next.splice(to > from ? to - 1 : to, 0, row);
  return next;
}

/** A commit of the result: what the rebase will leave, in order. */
export interface PreviewCommit {
  /** The commits that make it up: the first one, then those folded into it. */
  ids: string[];
  title: string;
  /** Messages of folded commits that a squash keeps next to the title. */
  extra: string[];
  reworded: boolean;
  /** The rebase stops after this commit so it can be changed. */
  edit: boolean;
}

const firstLine = (text: string): string => text.trim().split("\n")[0];

/**
 * The commits as they will be once the list has run: dropped ones gone, squashed and fixed-up
 * ones folded into the commit above, rewords with their new message. Nothing is run to get it.
 */
export function previewOf(rows: TodoRow[]): PreviewCommit[] {
  const result: PreviewCommit[] = [];
  for (const row of rows) {
    if (row.action === "drop") continue;
    if (row.action === "squash" || row.action === "fixup") {
      const target = result[result.length - 1];
      if (!target) continue; // The list is refused: there is nothing above to fold into.
      target.ids.push(row.id);
      if (row.action === "squash") {
        // A combined message replaces the messages; without one git keeps them all.
        if (row.message.trim()) {
          target.title = firstLine(row.message);
          target.extra = [];
          target.reworded = true;
        } else {
          target.extra.push(row.summary);
        }
      }
      continue;
    }
    const reworded = row.action === "reword" && row.message.trim() !== "";
    result.push({
      ids: [row.id],
      title: reworded ? firstLine(row.message) : row.summary,
      extra: [],
      reworded,
      edit: row.action === "edit",
    });
  }
  return result;
}
