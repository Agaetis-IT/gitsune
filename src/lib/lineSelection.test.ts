import { describe, expect, it } from "vitest";
import type { DiffHunk } from "./api";
import { canPickLines, changeIndices, sortedSelection, toggled, toggledAll } from "./lineSelection";

const line = (kind: "context" | "add" | "delete", noEol = false) => ({
  kind,
  oldNo: null,
  newNo: null,
  text: "x",
  noEol,
});

const hunk = (lines: ReturnType<typeof line>[]): DiffHunk => ({
  id: "h",
  oldStart: 1,
  oldLines: 1,
  newStart: 1,
  newLines: 1,
  header: "@@ -1,1 +1,1 @@",
  lines,
});

describe("line selection", () => {
  const h = hunk([line("context"), line("delete"), line("add"), line("context"), line("add")]);

  it("finds the changes among the lines", () => {
    expect(changeIndices(h)).toEqual([1, 2, 4]);
  });

  it("toggles one line without touching the selection it was given", () => {
    const start = new Set([1]);
    expect([...toggled(start, 2)]).toEqual([1, 2]);
    expect([...toggled(start, 1)]).toEqual([]);
    expect([...start]).toEqual([1]);
  });

  it("selects every change, then none", () => {
    const all = toggledAll(h, new Set());
    expect(sortedSelection(all)).toEqual([1, 2, 4]);
    expect(sortedSelection(toggledAll(h, new Set([1])))).toEqual([1, 2, 4]);
    expect(toggledAll(h, all).size).toBe(0);
  });

  it("refuses single lines in a hunk with a line that has no ending", () => {
    expect(canPickLines(h)).toBe(true);
    expect(canPickLines(hunk([line("add", true)]))).toBe(false);
  });
});
