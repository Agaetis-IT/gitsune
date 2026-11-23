import type { DiffHunk, DiffLineKind } from "@/lib/api";

export interface SplitCell {
  lineNo: number | null;
  text: string;
  kind: DiffLineKind;
  noEol: boolean;
}

/** One row of the side-by-side view; `null` is an empty filler cell. */
export interface SplitRow {
  left: SplitCell | null;
  right: SplitCell | null;
}

/**
 * Lays a hunk out in two columns: context on both sides, and each block of changes
 * with its deleted lines on the left facing its added lines on the right, row by row.
 */
export function toSplitRows(hunk: DiffHunk): SplitRow[] {
  const rows: SplitRow[] = [];
  let deleted: SplitCell[] = [];
  let added: SplitCell[] = [];

  const flush = () => {
    const height = Math.max(deleted.length, added.length);
    for (let i = 0; i < height; i++) {
      rows.push({ left: deleted[i] ?? null, right: added[i] ?? null });
    }
    deleted = [];
    added = [];
  };

  for (const line of hunk.lines) {
    if (line.kind === "context") {
      flush();
      rows.push({
        left: { lineNo: line.oldNo, text: line.text, kind: "context", noEol: line.noEol },
        right: { lineNo: line.newNo, text: line.text, kind: "context", noEol: line.noEol },
      });
    } else if (line.kind === "delete") {
      deleted.push({ lineNo: line.oldNo, text: line.text, kind: "delete", noEol: line.noEol });
    } else {
      added.push({ lineNo: line.newNo, text: line.text, kind: "add", noEol: line.noEol });
    }
  }
  flush();
  return rows;
}
