import { describe, expect, it } from "vitest";
import type { RefInfo, Worktree } from "./api";
import {
  checkedOutElsewhere,
  defaultWorktreePath,
  flatten,
  folderName,
  freeBranches,
  parentFolder,
  samePath,
  worktreeOfRefusal,
} from "./worktrees";

const tree = (path: string, branch: string | null, isMain = false): Worktree => ({
  path,
  head: "a".repeat(40),
  branch,
  detached: branch === null,
  bare: false,
  isMain,
  locked: null,
  prunable: null,
  dirty: false,
});

const local = (name: string): RefInfo => ({
  name,
  fullName: `refs/heads/${name}`,
  kind: "local",
  target: "a".repeat(40),
  isHead: false,
});

describe("paths", () => {
  it("compares folders without their trailing slash", () => {
    expect(samePath("/work/app/", "/work/app")).toBe(true);
    expect(samePath("/work/app", "/work/other")).toBe(false);
    expect(folderName("/work/app/")).toBe("app");
    expect(parentFolder("/work/app")).toBe("/work");
    expect(parentFolder("/app")).toBe("/");
  });

  it("proposes a folder next to the repository, named after the branch", () => {
    expect(flatten("feat/login page/5310")).toBe("feat-login-page-5310");
    expect(defaultWorktreePath("/work/app", "feat/login")).toBe("/work/app-feat-login");
    expect(defaultWorktreePath("/work/app/", "")).toBe("/work/app");
  });
});

describe("branches and working trees", () => {
  const trees = [tree("/work/app", "main", true), tree("/work/app-x", "feat/x")];

  it("finds the other working tree that holds a branch", () => {
    expect(checkedOutElsewhere(trees, "/work/app", "feat/x")?.path).toBe("/work/app-x");
    expect(checkedOutElsewhere(trees, "/work/app", "main")).toBeUndefined();
    expect(checkedOutElsewhere(trees, "/work/app-x", "main")?.path).toBe("/work/app");
  });

  it("lists the branches that are free for a new working tree", () => {
    expect(freeBranches([local("main"), local("feat/x"), local("topic")], trees)).toEqual([
      "topic",
    ]);
  });
});

describe("worktreeOfRefusal", () => {
  it("reads the folder from both wordings git uses", () => {
    expect(worktreeOfRefusal("fatal: 'main' is already checked out at '/work/app'")).toBe(
      "/work/app",
    );
    expect(worktreeOfRefusal("fatal: 'x' is already used by worktree at '/work/x'")).toBe(
      "/work/x",
    );
    expect(worktreeOfRefusal("error: something else")).toBeNull();
  });
});
