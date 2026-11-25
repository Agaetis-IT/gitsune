import { describe, expect, it } from "vitest";
import type { BranchPr } from "./api";
import { prBadge, prBadges } from "./prStatus";

const pr = (overrides: Partial<BranchPr> = {}): BranchPr => ({
  branch: "feat/x",
  id: 12,
  title: "Add x",
  draft: false,
  review: "none",
  checks: "none",
  ...overrides,
});

describe("prBadge", () => {
  it("shows a failing build before anything else", () => {
    const badge = prBadge(pr({ checks: "failure", review: "approved" }));
    expect(badge.icon).toBe("x");
    expect(badge.title).toContain("build failing");
  });

  it("puts changes requested before a running build", () => {
    expect(prBadge(pr({ review: "changesRequested", checks: "pending" })).icon).toBe("x");
  });

  it("tells running, passing and approved apart", () => {
    expect(prBadge(pr({ checks: "pending" })).icon).toBe("clock");
    expect(prBadge(pr({ checks: "success" })).icon).toBe("check");
    expect(prBadge(pr({ review: "approved" })).icon).toBe("check");
  });

  it("falls back to the plain pull request icon, grey when it is a draft", () => {
    const open = prBadge(pr());
    const draft = prBadge(pr({ draft: true }));
    expect(open.icon).toBe("git-pull-request");
    expect(draft.icon).toBe("git-pull-request");
    expect(draft.color).not.toBe(open.color);
    expect(draft.title).toContain("draft");
  });

  it("greys a badge that could not be refreshed", () => {
    const fresh = prBadge(pr({ checks: "success" }));
    const stale = prBadge(pr({ checks: "success" }), true);
    expect(stale.color).not.toBe(fresh.color);
    expect(stale.title).toContain("could not be refreshed");
  });
});

describe("prBadges", () => {
  it("is keyed by branch", () => {
    const map = prBadges([pr(), pr({ branch: "fix/y", id: 3 })]);
    expect([...map.keys()]).toEqual(["feat/x", "fix/y"]);
    expect(map.get("fix/y")?.id).toBe(3);
  });
});
