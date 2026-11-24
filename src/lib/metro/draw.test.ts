import { describe, expect, it } from "vitest";
import type { RefInfo } from "@/lib/api";
import { describeGroup } from "./draw";
import { groupRefs } from "./refGroups";

const ref = (kind: RefInfo["kind"], name: string, fullName: string): RefInfo => ({
  name,
  fullName,
  kind,
  target: "a",
  isHead: false,
});

describe("describeGroup", () => {
  it("says where a branch is: this computer and each remote, once", () => {
    const [group] = groupRefs([
      ref("local", "release/1", "refs/heads/release/1"),
      ref("remote", "origin/release/1", "refs/remotes/origin/release/1"),
      ref("remote", "upstream/release/1", "refs/remotes/upstream/release/1"),
    ]);
    expect(describeGroup(group)).toBe("release/1 (this computer, origin, upstream)");
  });

  it("names a remote-only branch and a tag", () => {
    const groups = groupRefs([
      ref("remote", "origin/fix/a", "refs/remotes/origin/fix/a"),
      ref("tag", "v1", "refs/tags/v1"),
    ]);
    expect(groups.map(describeGroup)).toEqual(["fix/a (origin)", "v1 (tag)"]);
  });
});
