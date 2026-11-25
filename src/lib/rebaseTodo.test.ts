import { describe, expect, it } from "vitest";
import {
  dropCount,
  isUntouched,
  moveRow,
  moveRowTo,
  previewOf,
  problem,
  rowsOf,
  toItems,
} from "./rebaseTodo";

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

describe("moveRowTo", () => {
  const names = (rows: ReturnType<typeof rowsOf>) => rows.map((r) => r.summary);

  it("drops a row into a gap, above or below where it was", () => {
    const rows = rowsOf(commits);
    expect(names(moveRowTo(rows, 2, 0))).toEqual(["three", "one", "two"]);
    expect(names(moveRowTo(rows, 0, 3))).toEqual(["two", "three", "one"]);
    expect(names(moveRowTo(rows, 0, 2))).toEqual(["two", "one", "three"]);
  });

  it("changes nothing when dropped where it is, or off the list", () => {
    const rows = rowsOf(commits);
    expect(moveRowTo(rows, 1, 1)).toBe(rows);
    expect(moveRowTo(rows, 1, 2)).toBe(rows);
    expect(moveRowTo(rows, 5, 0)).toBe(rows);
    expect(moveRowTo(rows, 0, 9)).toBe(rows);
  });
});

describe("previewOf", () => {
  const rows = () => rowsOf(commits);

  it("is the same list when nothing was changed", () => {
    const preview = previewOf(rows());
    expect(preview.map((c) => c.title)).toEqual(["one", "two", "three"]);
    expect(preview.every((c) => c.ids.length === 1 && !c.reworded && !c.edit)).toBe(true);
  });

  it("leaves dropped commits out", () => {
    const list = rows();
    list[1].action = "drop";
    expect(previewOf(list).map((c) => c.title)).toEqual(["one", "three"]);
  });

  it("folds a fixup into the commit above and keeps its message", () => {
    const list = rows();
    list[1].action = "fixup";
    const preview = previewOf(list);
    expect(preview).toHaveLength(2);
    expect(preview[0]).toMatchObject({ title: "one", extra: [] });
    expect(preview[0].ids).toEqual([commits[0].id, commits[1].id]);
  });

  it("keeps both messages of a squash, or uses the combined one", () => {
    const list = rows();
    list[1].action = "squash";
    expect(previewOf(list)[0]).toMatchObject({ title: "one", extra: ["two"] });
    list[1].message = "One and two\nlonger text";
    expect(previewOf(list)[0]).toMatchObject({ title: "One and two", extra: [], reworded: true });
  });

  it("shows the new message of a reword, and marks an edit", () => {
    const list = rows();
    list[0].action = "reword";
    list[0].message = " Better title ";
    list[2].action = "edit";
    const preview = previewOf(list);
    expect(preview[0]).toMatchObject({ title: "Better title", reworded: true });
    expect(preview[2]).toMatchObject({ title: "three", edit: true });
    list[0].message = "";
    expect(previewOf(list)[0]).toMatchObject({ title: "one", reworded: false });
  });

  it("skips a fold that has nothing above it (the list is refused anyway)", () => {
    const list = rows();
    list[0].action = "squash";
    expect(previewOf(list).map((c) => c.title)).toEqual(["two", "three"]);
  });
});
