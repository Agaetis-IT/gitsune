import { describe, expect, it } from "vitest";
import type { DiffHunk, DiffLine } from "@/lib/api";
import { toSplitRows } from "./diffRows";

let oldNo = 1;
let newNo = 1;
const line = (kind: DiffLine["kind"], text: string): DiffLine => ({
  kind,
  oldNo: kind === "add" ? null : oldNo++,
  newNo: kind === "delete" ? null : newNo++,
  text,
  noEol: false,
});

function hunkOf(...lines: DiffLine[]): DiffHunk {
  return { id: "test", oldStart: 1, oldLines: 0, newStart: 1, newLines: 0, header: "", lines };
}

const texts = (rows: ReturnType<typeof toSplitRows>) =>
  rows.map((r) => [r.left?.text ?? null, r.right?.text ?? null]);

describe("toSplitRows", () => {
  it("shows context on both sides", () => {
    oldNo = newNo = 1;
    const rows = toSplitRows(hunkOf(line("context", "a"), line("context", "b")));
    expect(texts(rows)).toEqual([
      ["a", "a"],
      ["b", "b"],
    ]);
    expect(rows[1].left?.lineNo).toBe(2);
    expect(rows[1].right?.lineNo).toBe(2);
  });

  it("faces each deleted line with the added line that replaces it", () => {
    oldNo = newNo = 1;
    const rows = toSplitRows(
      hunkOf(
        line("context", "keep"),
        line("delete", "old 1"),
        line("delete", "old 2"),
        line("add", "new 1"),
        line("add", "new 2"),
        line("context", "tail"),
      ),
    );
    expect(texts(rows)).toEqual([
      ["keep", "keep"],
      ["old 1", "new 1"],
      ["old 2", "new 2"],
      ["tail", "tail"],
    ]);
    expect(rows[1].left?.kind).toBe("delete");
    expect(rows[1].right?.kind).toBe("add");
  });

  it("leaves a filler when one side has more lines", () => {
    oldNo = newNo = 1;
    const rows = toSplitRows(
      hunkOf(line("delete", "gone"), line("add", "a"), line("add", "b"), line("add", "c")),
    );
    expect(texts(rows)).toEqual([
      ["gone", "a"],
      [null, "b"],
      [null, "c"],
    ]);
    const onlyDeleted = toSplitRows(hunkOf(line("delete", "x")));
    expect(texts(onlyDeleted)).toEqual([["x", null]]);
  });

  it("handles an empty hunk", () => {
    expect(toSplitRows(hunkOf())).toEqual([]);
  });
});
