import { describe, expect, it } from "vitest";
import { landingIndex, moveId } from "./tabOrder";

describe("moveId", () => {
  it("moves a tab forward and backward", () => {
    expect(moveId(["a", "b", "c", "d"], 0, 2)).toEqual(["b", "c", "a", "d"]);
    expect(moveId(["a", "b", "c", "d"], 3, 1)).toEqual(["a", "d", "b", "c"]);
  });

  it("changes nothing when the move makes no sense", () => {
    const ids = ["a", "b"];
    expect(moveId(ids, 0, 0)).toEqual(ids);
    expect(moveId(ids, -1, 1)).toEqual(ids);
    expect(moveId(ids, 0, 2)).toEqual(ids);
    expect(moveId(ids, 5, 0)).toEqual(ids);
  });

  it("returns a new list", () => {
    const ids = ["a", "b"];
    expect(moveId(ids, 0, 0)).not.toBe(ids);
  });
});

describe("landingIndex", () => {
  it("turns a gap into the index the tab ends up at", () => {
    expect(landingIndex(0, 0)).toBe(0);
    expect(landingIndex(0, 1)).toBe(0);
    expect(landingIndex(0, 3)).toBe(2);
    expect(landingIndex(3, 1)).toBe(1);
    expect(landingIndex(3, 4)).toBe(3);
  });
});
