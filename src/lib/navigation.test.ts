import { describe, expect, it } from "vitest";
import type { RefInfo } from "@/lib/api";
import { headSide, pickCheckoutRef } from "./navigation";

const ref = (name: string, kind: RefInfo["kind"], isHead = false): RefInfo => ({
  name,
  fullName: `refs/${name}`,
  kind,
  target: "0".repeat(40),
  isHead,
});

describe("what a double click switches to", () => {
  it("a local branch at the tip", () => {
    expect(pickCheckoutRef([ref("feature/x", "local")])?.name).toBe("feature/x");
  });

  it("prefers the local branch over the remote one on the same commit", () => {
    const picked = pickCheckoutRef([ref("origin/x", "remote"), ref("x", "local")]);
    expect(picked).toMatchObject({ name: "x", kind: "local" });
  });

  it("takes the branch that is not checked out when several sit on the commit", () => {
    expect(pickCheckoutRef([ref("main", "local", true), ref("other", "local")])?.name).toBe(
      "other",
    );
  });

  it("returns the checked-out branch when it is the only one, so the app can say so", () => {
    expect(pickCheckoutRef([ref("main", "local", true)])).toMatchObject({ isHead: true });
  });

  it("a remote-only tip gives the remote branch", () => {
    expect(pickCheckoutRef([ref("origin/x", "remote")])).toMatchObject({ kind: "remote" });
  });

  it("tags and commits without a branch do nothing", () => {
    expect(pickCheckoutRef([ref("v1", "tag")])).toBeNull();
    expect(pickCheckoutRef([])).toBeNull();
  });
});

describe("where HEAD is", () => {
  it("is nowhere while the rows in view or HEAD are unknown", () => {
    expect(headSide(null, [0, 10])).toBeNull();
    expect(headSide(5, null)).toBeNull();
  });

  it("is above, below, or in view", () => {
    expect(headSide(2, [10, 30])).toBe("above");
    expect(headSide(50, [10, 30])).toBe("below");
    expect(headSide(10, [10, 30])).toBeNull();
    expect(headSide(30, [10, 30])).toBeNull();
  });
});
