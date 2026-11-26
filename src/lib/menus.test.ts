import { describe, expect, it } from "vitest";
import type { RefInfo } from "@/lib/api";
import { commitMenu, laneMenu, type MenuContext, pullRequestTargetOf, refMenu } from "./menus";

const ctx = (overrides: Partial<MenuContext> = {}): MenuContext => ({
  readOnly: false,
  headId: "head",
  detached: false,
  ...overrides,
});

const ref = (name: string, kind: RefInfo["kind"], isHead = false): RefInfo => ({
  name,
  fullName: `refs/${name}`,
  kind,
  target: "t",
  isHead,
});

const ids = (items: { id: string }[]) => items.map((i) => i.id);
const find = (items: ReturnType<typeof refMenu>, id: string) => items.find((i) => i.id === id);

describe("commit menu", () => {
  it("offers to check out, branch, tag and copy", () => {
    const items = commitMenu({ id: "c1", author: "Ada" }, ctx());
    expect(ids(items)).toEqual([
      "checkoutCommit",
      "createBranch",
      "createTag",
      "cherryPick",
      "revert",
      "reset",
      "rebase",
      "rebaseInteractive",
      "focusAuthor",
      "copyId",
    ]);
    expect(items.every((i) => !i.disabled)).toBe(true);
  });

  it("offers to focus on the author, or to remove the focus", () => {
    const commit = { id: "c1", author: "Ada Lovelace" };
    const label = (authorFocused: boolean) =>
      find(commitMenu(commit, ctx({ authorFocused })), "focusAuthor")?.label;
    expect(label(false)).toBe("Focus on Ada Lovelace");
    expect(label(true)).toBe("Remove the author focus");
  });

  it("explains why a commit cannot be checked out when HEAD is already detached there", () => {
    const items = commitMenu({ id: "head", author: "Ada" }, ctx({ detached: true }));
    expect(items[0].disabled).toContain("already");
    // On a branch whose tip is that commit, detaching is allowed.
    expect(
      commitMenu({ id: "head", author: "Ada" }, ctx({ detached: false }))[0].disabled,
    ).toBeUndefined();
  });

  it("greys out the writes in read-only mode but still allows copying", () => {
    const items = commitMenu({ id: "c1", author: "Ada" }, ctx({ readOnly: true }));
    expect(items.filter((i) => i.disabled).map((i) => i.id)).toEqual([
      "checkoutCommit",
      "createBranch",
      "createTag",
      "cherryPick",
      "revert",
      "reset",
      "rebase",
      "rebaseInteractive",
    ]);
    expect(items.find((i) => i.id === "copyId")?.disabled).toBeUndefined();
    expect(items[0].disabled).toBe("Read-only mode is on.");
  });
});

describe("commit menu, pull request", () => {
  it("offers a pull request into the branch at the commit", () => {
    const items = commitMenu({ id: "c1", author: "Ada" }, ctx({ pullRequestTarget: "develop" }));
    const item = items.find((i) => i.id === "pullRequestHere");
    expect(item?.label).toBe("Create a pull request into develop…");
    expect(item?.disabled).toBeUndefined();
  });

  it("offers nothing for a commit without a branch, and waits for a branch when detached", () => {
    expect(ids(commitMenu({ id: "c1", author: "Ada" }, ctx()))).not.toContain("pullRequestHere");
    const detached = commitMenu(
      { id: "c1", author: "Ada" },
      ctx({ pullRequestTarget: "develop", detached: true }),
    );
    expect(detached.find((i) => i.id === "pullRequestHere")?.disabled).toMatch(/detached/);
  });

  it("takes the branch name of a local or a remote ref, never the current branch", () => {
    const refs = [
      ref("v1", "tag"),
      ref("origin/HEAD", "remote"),
      ref("feat/x", "local", true),
      ref("origin/develop", "remote"),
    ];
    expect(pullRequestTargetOf(refs, "feat/x")).toBe("develop");
    expect(pullRequestTargetOf([ref("v1", "tag")], "feat/x")).toBeNull();
    expect(pullRequestTargetOf([ref("feat/x", "local")], "feat/x")).toBeNull();
  });
});

describe("commit menu, history rewriting", () => {
  it("cherry-picking the current commit onto itself is refused with a reason", () => {
    const items = commitMenu({ id: "head", author: "Ada" }, ctx({ headId: "head" }));
    expect(find(items, "cherryPick")?.disabled).toContain("already");
    expect(
      find(commitMenu({ id: "other", author: "Ada" }, ctx({ headId: "head" })), "cherryPick")
        ?.disabled,
    ).toBeUndefined();
  });

  it("a reset is shown as a destructive action", () => {
    expect(find(commitMenu({ id: "c1", author: "Ada" }, ctx()), "reset")?.danger).toBe(true);
  });
});

describe("stash label menu", () => {
  it("applies, pops or drops the stash, and nothing about branches", () => {
    const items = refMenu(ref("stash@{0}", "stash"), ctx());
    expect(ids(items)).toEqual(["stashApply", "stashPop", "stashDrop"]);
    expect(find(items, "stashDrop")?.danger).toBe(true);
  });

  it("greys out the three actions in read-only mode", () => {
    const items = refMenu(ref("stash@{0}", "stash"), ctx({ readOnly: true }));
    expect(items.every((i) => i.disabled)).toBe(true);
  });

  it("is never taken for a branch a pull request could go into", () => {
    expect(pullRequestTargetOf([ref("stash@{0}", "stash")], "feat/x")).toBeNull();
    expect(
      pullRequestTargetOf([ref("stash@{0}", "stash"), ref("develop", "local")], "feat/x"),
    ).toBe("develop");
  });
});

describe("ref menu", () => {
  it("a local branch can be switched to, renamed, re-tracked and deleted", () => {
    const items = refMenu(ref("topic", "local"), ctx({ canHighlight: true }));
    expect(ids(items)).toEqual([
      "checkout",
      "rename",
      "upstream",
      "push",
      "merge",
      "rebase",
      "reset",
      "pullRequestInto",
      "delete",
      "highlight",
      "hide",
      "copyName",
    ]);
    expect(find(items, "delete")?.danger).toBe(true);
  });

  it("offers to reset the checked-out branch to the branch that is clicked", () => {
    const local = find(refMenu(ref("develop", "local"), ctx({ headBranch: "feat/x" })), "reset");
    expect(local?.label).toBe("Reset feat/x to develop…");
    expect(local?.danger).toBe(true);
    expect(local?.disabled).toBeUndefined();
    const remote = find(
      refMenu(ref("origin/develop", "remote"), ctx({ headBranch: "feat/x" })),
      "reset",
    );
    expect(remote?.label).toBe("Reset feat/x to origin/develop…");
    expect(remote?.disabled).toBeUndefined();
  });

  it("does not offer to reset a detached HEAD or the branch to itself", () => {
    const detached = refMenu(ref("develop", "local"), ctx({ detached: true, headBranch: null }));
    expect(find(detached, "reset")?.disabled).toMatch(/detached/);
    const self = refMenu(ref("main", "local", true), ctx({ headBranch: "main" }));
    expect(find(self, "reset")?.disabled).toContain("current branch");
  });

  it("offers a pull request from the current branch into the branch that is clicked", () => {
    const local = find(
      refMenu(ref("develop", "local"), ctx({ headBranch: "feat/x" })),
      "pullRequestInto",
    );
    expect(local?.label).toBe("Create a pull request into develop…");
    expect(local?.disabled).toBeUndefined();
    const remote = find(
      refMenu(ref("origin/develop", "remote"), ctx({ headBranch: "feat/x" })),
      "pullRequestInto",
    );
    expect(remote?.label).toBe("Create a pull request into develop…");
    expect(remote?.disabled).toBeUndefined();
  });

  it("does not offer a pull request into a branch without a checked-out branch", () => {
    const detached = refMenu(ref("develop", "local"), ctx({ detached: true, headBranch: null }));
    expect(find(detached, "pullRequestInto")?.disabled).toMatch(/detached/);
  });

  it("the current branch has one pull request entry: open it when it exists, else create", () => {
    const create = refMenu(ref("main", "local", true), ctx({ headBranch: "main" }));
    expect(ids(create).filter((id) => id.startsWith("pullRequest"))).toEqual(["pullRequest"]);
    expect(find(create, "pullRequest")?.label).toBe("Create a pull request…");
    const open = refMenu(
      ref("main", "local", true),
      ctx({ headBranch: "main", hasPullRequest: true }),
    );
    expect(ids(open).filter((id) => id.startsWith("pullRequest"))).toEqual(["pullRequest"]);
    expect(find(open, "pullRequest")?.label).toBe("Open the pull request");
  });

  it("another branch offers to start a pull request into it, and to open its own when it has one", () => {
    const without = refMenu(ref("develop", "local"), ctx({ headBranch: "feat/x" }));
    expect(ids(without).filter((id) => id.startsWith("pullRequest"))).toEqual(["pullRequestInto"]);
    const withOne = refMenu(
      ref("origin/develop", "remote"),
      ctx({ headBranch: "feat/x", hasPullRequest: true }),
    );
    expect(ids(withOne).filter((id) => id.startsWith("pullRequest"))).toEqual([
      "pullRequestInto",
      "pullRequest",
    ]);
    expect(find(withOne, "pullRequest")?.label).toBe("Open the pull request of develop");
  });

  it("a pull request can be started or opened even in read-only mode", () => {
    const current = refMenu(
      ref("main", "local", true),
      ctx({ readOnly: true, headBranch: "main" }),
    );
    expect(find(current, "pullRequest")?.disabled).toBeUndefined();
    const other = refMenu(
      ref("origin/topic", "remote"),
      ctx({ readOnly: true, headBranch: "main" }),
    );
    expect(find(other, "pullRequestInto")?.disabled).toBeUndefined();
  });

  it("the checked-out branch cannot be merged into itself", () => {
    const items = refMenu(ref("main", "local", true), ctx());
    expect(find(items, "merge")?.disabled).toContain("current branch");
    expect(find(items, "rebase")?.disabled).toContain("current branch");
    expect(find(refMenu(ref("topic", "local"), ctx()), "merge")?.label).toContain("topic");
  });

  it("the checked-out branch cannot be switched to or deleted, and says why", () => {
    const items = refMenu(ref("main", "local", true), ctx());
    expect(find(items, "checkout")?.disabled).toContain("already checked out");
    expect(find(items, "delete")?.disabled).toContain("cannot be deleted");
    expect(find(items, "rename")?.disabled).toBeUndefined();
  });

  it("a remote branch is checked out as a local one and can be hidden", () => {
    const items = refMenu(ref("origin/topic", "remote"), ctx());
    expect(ids(items)).toEqual([
      "checkout",
      "merge",
      "rebase",
      "reset",
      "pullRequestInto",
      "deleteRemote",
      "hide",
      "copyName",
    ]);
    expect(find(items, "checkout")?.label).toContain("local branch");
    expect(find(items, "deleteRemote")?.danger).toBe(true);
    expect(
      find(refMenu(ref("origin/topic", "remote"), ctx({ readOnly: true })), "deleteRemote")
        ?.disabled,
    ).toBeTruthy();
  });

  it("a tag can be checked out (detached) and deleted", () => {
    const items = refMenu(ref("v1", "tag"), ctx());
    expect(ids(items)).toEqual(["checkout", "pushTag", "deleteTag", "copyName"]);
    expect(find(items, "deleteTag")?.danger).toBe(true);
  });

  it("pushing a tag needs a repository that can be written to", () => {
    expect(find(refMenu(ref("v1", "tag"), ctx()), "pushTag")?.disabled).toBeUndefined();
    expect(find(refMenu(ref("v1", "tag"), ctx({ readOnly: true })), "pushTag")?.disabled).toBe(
      "Read-only mode is on.",
    );
  });

  it("the highlight and hide entries follow the current state", () => {
    const on = refMenu(
      ref("topic", "local"),
      ctx({ canHighlight: true, highlighted: true, hidden: true }),
    );
    expect(find(on, "highlight")?.label).toBe("Remove the highlight");
    expect(find(on, "hide")?.label).toBe("Show in the graph");
    expect(find(refMenu(ref("topic", "local"), ctx()), "highlight")).toBeUndefined();
  });

  it("read-only mode greys out every write of a branch menu but not the view actions", () => {
    const items = refMenu(ref("topic", "local"), ctx({ readOnly: true, canHighlight: true }));
    const disabled = items.filter((i) => i.disabled).map((i) => i.id);
    expect(disabled).toEqual([
      "checkout",
      "rename",
      "upstream",
      "push",
      "merge",
      "rebase",
      "reset",
      "delete",
    ]);
    expect(find(items, "hide")?.disabled).toBeUndefined();
  });
});

describe("lane menu", () => {
  it("highlights, and hides only a lane that has a ref", () => {
    expect(ids(laneMenu({ ...ctx(), canHide: true }))).toEqual(["highlight", "hide"]);
    expect(ids(laneMenu({ ...ctx(), canHide: false }))).toEqual(["highlight"]);
    expect(laneMenu({ ...ctx(), canHide: false, highlighted: true })[0].label).toBe(
      "Remove the highlight",
    );
  });

  it("offers to open the ticket named in the branch, local or remote", () => {
    const local = refMenu(ref("feat/login/5310", "local"), ctx());
    expect(find(local, "openTicket")?.label).toBe("Open ticket 5310");
    const remote = refMenu(ref("origin/feat/login/5310", "remote"), ctx());
    expect(find(remote, "openTicket")?.label).toBe("Open ticket 5310");
    expect(find(refMenu(ref("main", "local"), ctx()), "openTicket")).toBeUndefined();
  });
});
