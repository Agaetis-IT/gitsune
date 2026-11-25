import { describe, expect, it } from "vitest";
import {
  type Choice,
  compose,
  resultLineOf,
  sideText,
  conflictsOf,
  hasMarkers,
  parseConflicts,
  resolveBlock,
  unresolvedCount,
} from "./conflicts";

const TWO_SIDES = "top\n<<<<<<< HEAD\nmain line\n=======\nside line\n>>>>>>> side\nbottom\n";

const THREE_SIDES = "<<<<<<< HEAD\nmain\n||||||| base\noriginal\n=======\nside\n>>>>>>> side\n";

describe("parsing conflict markers", () => {
  it("splits a file into text and conflict blocks", () => {
    const segments = parseConflicts(TWO_SIDES);
    expect(segments).toHaveLength(3);
    expect(segments[0]).toEqual({ type: "text", text: "top\n" });
    expect(segments[1]).toMatchObject({
      type: "conflict",
      id: 0,
      ours: "main line\n",
      theirs: "side line\n",
      base: null,
      oursLabel: "HEAD",
      theirsLabel: "side",
    });
    expect(segments[2]).toEqual({ type: "text", text: "bottom\n" });
  });

  it("reads the common ancestor when git wrote it (diff3 style)", () => {
    const [block] = conflictsOf(parseConflicts(THREE_SIDES));
    expect(block.base).toBe("original\n");
    expect(block.ours).toBe("main\n");
    expect(block.theirs).toBe("side\n");
  });

  it("numbers several blocks in order", () => {
    const text = `${TWO_SIDES}middle\n${TWO_SIDES}`;
    expect(conflictsOf(parseConflicts(text)).map((b) => b.id)).toEqual([0, 1]);
  });

  it("keeps Windows line endings and a last line without one", () => {
    const text = "a\r\n<<<<<<< HEAD\r\nours\r\n=======\r\ntheirs\r\n>>>>>>> x\r\nlast";
    const segments = parseConflicts(text);
    expect(conflictsOf(segments)[0].ours).toBe("ours\r\n");
    expect(compose(segments, { 0: "theirs" })).toBe("a\r\ntheirs\r\nlast");
  });

  it("an empty side is a side that deleted the lines", () => {
    const [block] = conflictsOf(parseConflicts("<<<<<<< HEAD\n=======\nadded\n>>>>>>> s\n"));
    expect(block.ours).toBe("");
    expect(block.theirs).toBe("added\n");
  });

  it("a marker that is never closed stays as text instead of losing lines", () => {
    const text = "a\n<<<<<<< HEAD\nb\nc\n";
    const segments = parseConflicts(text);
    expect(conflictsOf(segments)).toHaveLength(0);
    expect(compose(segments, {})).toBe(text);
  });

  it("a file without conflicts is one block of text, and an empty file has none", () => {
    expect(parseConflicts("just text\n")).toEqual([{ type: "text", text: "just text\n" }]);
    expect(parseConflicts("")).toEqual([]);
  });

  it("lines that merely contain marker characters are not markers", () => {
    const text = "x <<<<<<< y\n======= not alone\n";
    expect(hasMarkers(text)).toBe(false);
  });
});

describe("building the result", () => {
  const segments = parseConflicts(TWO_SIDES);

  it("takes ours, theirs, or both in either order", () => {
    expect(compose(segments, { 0: "ours" })).toBe("top\nmain line\nbottom\n");
    expect(compose(segments, { 0: "theirs" })).toBe("top\nside line\nbottom\n");
    expect(compose(segments, { 0: "both" })).toBe("top\nmain line\nside line\nbottom\n");
    expect(compose(segments, { 0: "bothReversed" })).toBe("top\nside line\nmain line\nbottom\n");
  });

  it("joins two sides that do not end with a line break", () => {
    const [block] = conflictsOf(
      parseConflicts("<<<<<<< a\nfirst\n=======\nsecond\n>>>>>>> b\n").map((s) =>
        s.type === "conflict" ? { ...s, ours: "first", theirs: "second" } : s,
      ),
    );
    expect(resolveBlock(block, "both")).toBe("first\nsecond");
  });

  it("an unresolved block keeps its markers, so nothing is lost on the way", () => {
    const text = compose(segments, {});
    expect(text).toContain("<<<<<<< HEAD");
    expect(text).toContain(">>>>>>> side");
    expect(hasMarkers(text)).toBe(true);
    expect(parseConflicts(text)).toEqual(
      parseConflicts(TWO_SIDES).map((s) => (s.type === "conflict" ? { ...s } : s)),
    );
  });

  it("counts what is still to decide", () => {
    const two = parseConflicts(`${TWO_SIDES}${TWO_SIDES}`);
    const choices: Record<number, Choice> = { 0: "ours" };
    expect(unresolvedCount(two, choices)).toBe(1);
    choices[1] = "theirs";
    expect(unresolvedCount(two, choices)).toBe(0);
    expect(hasMarkers(compose(two, choices))).toBe(false);
  });
});

describe("following a conflict in the result", () => {
  const text =
    "a\nb\n<<<<<<< HEAD\nx\ny\n=======\nz\n>>>>>>> side\nc\n<<<<<<< HEAD\nm\n=======\nn\n>>>>>>> side\n";

  it("tells the line where each block starts, whatever was chosen before it", () => {
    const segments = parseConflicts(text);
    expect(resultLineOf(segments, {}, 0)).toBe(2);
    expect(resultLineOf(segments, {}, 1)).toBe(2 + 6 + 1);
    // The first block settled with its two lines of ours: the second starts earlier.
    expect(resultLineOf(segments, { 0: "ours" }, 1)).toBe(2 + 2 + 1);
    expect(resultLineOf(segments, { 0: "theirs" }, 1)).toBe(2 + 1 + 1);
  });

  it("is 0 at the top of the file and null for a block that does not exist", () => {
    const segments = parseConflicts("<<<<<<< HEAD\na\n=======\nb\n>>>>>>> side\n");
    expect(resultLineOf(segments, {}, 0)).toBe(0);
    expect(resultLineOf(segments, {}, 7)).toBeNull();
  });
});

describe("a whole side of the file", () => {
  it("keeps the plain text and takes one side of every block", () => {
    const segments = parseConflicts(TWO_SIDES);
    expect(sideText(segments, "ours")).toBe("top\nmain line\nbottom\n");
    expect(sideText(segments, "theirs")).toBe("top\nside line\nbottom\n");
  });
});
