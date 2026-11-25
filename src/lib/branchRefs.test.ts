import { describe, expect, it } from "vitest";
import type { RefInfo } from "@/lib/api";
import { refsOfBranch, stripRemote } from "./branchRefs";

const ref = (name: string, kind: RefInfo["kind"]): RefInfo => ({
  name,
  fullName: `${kind}:${name}`,
  kind,
  target: "0".repeat(40),
  isHead: false,
});

const REFS = [
  ref("main", "local"),
  ref("feature/x", "local"),
  ref("origin/main", "remote"),
  ref("origin/feature/x", "remote"),
  ref("upstream/main", "remote"),
  ref("v1", "tag"),
  ref("main", "tag"),
];

describe("stripRemote", () => {
  it("removes the first path segment only", () => {
    expect(stripRemote("origin/feature/x")).toBe("feature/x");
    expect(stripRemote("origin/main")).toBe("main");
    expect(stripRemote("main")).toBe("main");
  });
});

describe("refsOfBranch", () => {
  it("groups the local branch with its remote-tracking branches", () => {
    const names = refsOfBranch(REFS, "main").map((r) => r.fullName);
    expect(names).toEqual(["local:main", "remote:origin/main", "remote:upstream/main"]);
  });

  it("finds the group from a remote name too", () => {
    expect(refsOfBranch(REFS, "origin/feature/x").map((r) => r.name)).toEqual([
      "feature/x",
      "origin/feature/x",
    ]);
  });

  it("leaves tags alone, and unknown names give nothing", () => {
    expect(refsOfBranch(REFS, "v1").map((r) => r.name)).toEqual(["v1"]);
    expect(refsOfBranch(REFS, "nope")).toEqual([]);
    expect(refsOfBranch([], "main")).toEqual([]);
  });
});
