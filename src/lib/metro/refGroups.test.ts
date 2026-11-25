import { describe, expect, it } from "vitest";
import type { RefInfo } from "@/lib/api";
import { groupRefs, remoteOf } from "./refGroups";

const local = (name: string, isHead = false): RefInfo => ({
  name,
  fullName: `refs/heads/${name}`,
  kind: "local",
  target: "a",
  isHead,
});
const remote = (name: string): RefInfo => ({
  name,
  fullName: `refs/remotes/${name}`,
  kind: "remote",
  target: "a",
  isHead: false,
});
const tag = (name: string): RefInfo => ({
  name,
  fullName: `refs/tags/${name}`,
  kind: "tag",
  target: "a",
  isHead: false,
});

describe("groupRefs", () => {
  it("puts a branch and its remote copies in one group", () => {
    const groups = groupRefs([
      remote("origin/feature/x"),
      local("feature/x"),
      remote("upstream/feature/x"),
    ]);
    expect(groups).toHaveLength(1);
    expect(groups[0].name).toBe("feature/x");
    expect(groups[0].local?.name).toBe("feature/x");
    expect(groups[0].remotes.map((r) => r.remote)).toEqual(["origin", "upstream"]);
    expect(groups[0].primary.kind).toBe("local");
  });

  it("keeps a remote-only branch with the remote ref as its target", () => {
    const [group] = groupRefs([remote("origin/fix/a")]);
    expect(group.name).toBe("fix/a");
    expect(group.local).toBeNull();
    expect(group.primary.fullName).toBe("refs/remotes/origin/fix/a");
  });

  it("leaves origin/HEAD out and lists the current branch first, tags last", () => {
    const groups = groupRefs([
      tag("v1"),
      remote("origin/HEAD"),
      remote("origin/other"),
      local("main", true),
      local("dev"),
    ]);
    expect(groups.map((g) => g.name)).toEqual(["main", "dev", "other", "v1"]);
    expect(groups[0].isHead).toBe(true);
  });

  it("tells the remote of a remote-tracking ref", () => {
    expect(remoteOf(remote("origin/a/b"))).toBe("origin");
  });
});
