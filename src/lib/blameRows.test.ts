import { describe, expect, it } from "vitest";
import type { Blame } from "./api";
import { blameRows, isDarkCssColor, relativeTime } from "./blameRows";

const commit = (id: string, author: string) => ({
  id: id.repeat(40),
  author,
  time: 1_700_000_000,
  summary: `by ${author}`,
  uncommitted: false,
});

describe("blameRows", () => {
  const blame: Blame = {
    commits: [commit("a", "Ada"), commit("b", "Bob")],
    lines: [
      { commit: 0, text: "one" },
      { commit: 0, text: "two" },
      { commit: 1, text: "three" },
      { commit: 0, text: "four" },
    ],
    truncated: false,
  };

  it("marks the start of each run of the same commit and counts its lines", () => {
    const rows = blameRows(blame);
    expect(rows.map((r) => r.first)).toEqual([true, false, true, true]);
    expect(rows.map((r) => r.span)).toEqual([2, 0, 1, 1]);
    expect(rows.map((r) => r.line)).toEqual([1, 2, 3, 4]);
    expect(rows[2].commit.author).toBe("Bob");
  });

  it("is empty for a file without lines", () => {
    expect(blameRows({ commits: [], lines: [], truncated: false })).toEqual([]);
  });
});

describe("relativeTime", () => {
  const now = 1_700_000_000_000;
  it("says how long ago, in the largest sensible unit", () => {
    expect(relativeTime(1_700_000_000 - 3 * 24 * 3600, now)).toBe("3 days ago");
    expect(relativeTime(1_700_000_000 - 2 * 3600, now)).toBe("2 hours ago");
    expect(relativeTime(1_700_000_000 - 400 * 24 * 3600, now)).toBe("last year");
    expect(relativeTime(1_700_000_000, now)).toBe("now");
  });
});

describe("isDarkCssColor", () => {
  it("reads rgb and hex colors", () => {
    expect(isDarkCssColor("rgb(10, 12, 20)")).toBe(true);
    expect(isDarkCssColor("rgba(255, 255, 255, 1)")).toBe(false);
    expect(isDarkCssColor("#101820")).toBe(true);
    expect(isDarkCssColor("white")).toBe(false);
  });
});
