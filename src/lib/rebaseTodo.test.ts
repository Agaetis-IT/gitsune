import { describe, expect, it } from "vitest";
import { dropCount, isUntouched, moveRow, problem, rowsOf, toItems } from "./rebaseTodo";

const commits = ["one", "two", "three"].map((summary, i) => ({
  id: String(i).repeat(40),
  summary,
}));

describe("the rebase list", () => {
  it("starts as one pick per commit, in order", () => {
    const rows = rowsOf(commits);
    expect(rows.map((r) => [r.summary, r.action])).toEqual([
      ["one", "pick"],
      ["two", "pick"],
      ["three", "pick"],
    ]);
    expect(isUntouched(rows, commits)).toBe(true);
  });

  it("moves a row up or down and ignores moves off the ends", () => {
    const rows = rowsOf(commits);
    expect(moveRow(rows, 2, -1).map((r) => r.summary)).toEqual(["one", "three", "two"]);
    expect(moveRow(rows, 0, 1).map((r) => r.summary)).toEqual(["two", "one", "three"]);
    expect(moveRow(rows, 0, -1)).toBe(rows);
    expect(moveRow(rows, 2, 1)).toBe(rows);
    expect(rows.map((r) => r.summary)).toEqual(["one", "two", "three"]);
  });

  it("is no longer plain once reordered, changed or given a message", () => {
    const rows = rowsOf(commits);
    expect(isUntouched(moveRow(rows, 0, 1), commits)).toBe(false);
    expect(isUntouched([{ ...rows[0], action: "drop" }, ...rows.slice(1)], commits)).toBe(false);
    expect(isUntouched([{ ...rows[0], message: "new" }, ...rows.slice(1)], commits)).toBe(false);
  });

  it("refuses to squash the first kept commit, even after a dropped one", () => {
    const rows = rowsOf(commits);
    expect(problem(rows)).toBeNull();
    expect(problem([{ ...rows[0], action: "squash" }, ...rows.slice(1)])).toContain("first");
    const dropped = [
      { ...rows[0], action: "drop" as const },
      { ...rows[1], action: "fixup" as const },
      rows[2],
    ];
    expect(problem(dropped)).toContain("first");
    expect(problem([rows[0], { ...rows[1], action: "squash" }, rows[2]])).toBeNull();
  });

  it("sends a message only for the actions that use one, and only when it is not empty", () => {
    const rows = rowsOf(commits).map((r, i) => ({
      ...r,
      action: (["reword", "pick", "squash"] as const)[i],
      message: "text",
    }));
    rows[2].message = "   ";
    expect(toItems(rows)).toEqual([
      { action: "reword", id: commits[0].id, message: "text" },
      { action: "pick", id: commits[1].id, message: null },
      { action: "squash", id: commits[2].id, message: null },
    ]);
  });

  it("counts the dropped commits", () => {
    const rows = rowsOf(commits);
    expect(dropCount(rows)).toBe(0);
    expect(
      dropCount([{ ...rows[0], action: "drop" }, rows[1], { ...rows[2], action: "drop" }]),
    ).toBe(2);
  });
});
