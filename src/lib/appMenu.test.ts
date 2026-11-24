import { describe, expect, it } from "vitest";
import {
  accelerators,
  type AppMenuState,
  buildMenuModel,
  MAX_MENU_BRANCHES,
  type MenuNode,
  parseMenuAction,
} from "./appMenu";

const STATE: AppMenuState = {
  hasRepo: true,
  canWrite: true,
  remoteBusy: false,
  readOnly: false,
  detached: false,
  hasHead: true,
  tabCount: 2,
  recents: [{ path: "/fake/alpha", name: "alpha" }],
  branches: [
    { name: "main", isHead: true },
    { name: "feature/x", isHead: false },
  ],
  density: "compact",
  activityOpen: false,
};

function search(nodes: readonly MenuNode[], label: string): MenuNode | undefined {
  for (const node of nodes) {
    if (node.kind === "separator" || node.kind === "predefined") continue;
    if (node.label === label) return node;
    if (node.kind === "submenu") {
      const inner = search(node.items, label);
      if (inner) return inner;
    }
  }
  return undefined;
}

function find(nodes: readonly MenuNode[], label: string): MenuNode {
  const node = search(nodes, label);
  if (!node) throw new Error(`no entry ${label}`);
  return node;
}

const enabled = (state: AppMenuState, label: string) => {
  const node = find(buildMenuModel(state), label);
  return node.kind === "item" || node.kind === "submenu" ? node.enabled !== false : true;
};

describe("application menu", () => {
  it("has the menus in the usual order", () => {
    const labels = buildMenuModel(STATE).map((n) => (n.kind === "submenu" ? n.label : ""));
    expect(labels).toEqual(["git-hydra", "File", "Edit", "Repository", "Branch", "View", "Window"]);
  });

  it("uses no accelerator twice", () => {
    const list = accelerators(buildMenuModel(STATE));
    expect(new Set(list).size).toBe(list.length);
  });

  it("greys what needs a repository when none is open", () => {
    const none = { ...STATE, hasRepo: false };
    for (const label of [
      "Fetch",
      "Refresh",
      "Close the Tab",
      "Search the Commits",
      "New Branch…",
    ]) {
      expect(enabled(none, label), label).toBe(false);
    }
    expect(enabled(none, "Open a Repository…")).toBe(true);
    expect(enabled(none, "Settings…")).toBe(true);
  });

  it("keeps reading actions in read-only mode and greys the writing ones", () => {
    const readOnly = { ...STATE, canWrite: false, readOnly: true };
    expect(enabled(readOnly, "Fetch")).toBe(true);
    expect(enabled(readOnly, "Refresh")).toBe(true);
    for (const label of ["Pull", "Push", "New Branch…", "Stash…", "Check Out"]) {
      expect(enabled(readOnly, label), label).toBe(false);
    }
  });

  it("greys the remote actions while one is running", () => {
    const busy = { ...STATE, remoteBusy: true };
    for (const label of ["Fetch", "Pull", "Push"]) expect(enabled(busy, label), label).toBe(false);
  });

  it("cannot merge or rebase on a detached HEAD, but can check out", () => {
    const detached = { ...STATE, detached: true };
    expect(enabled(detached, "Merge into the Current Branch")).toBe(false);
    expect(enabled(detached, "Rebase onto")).toBe(false);
    expect(enabled(detached, "Check Out")).toBe(true);
  });

  it("lists the other branches for merge and all of them, checked, for check out", () => {
    const model = buildMenuModel(STATE);
    const merge = find(model, "Merge into the Current Branch");
    const checkout = find(model, "Check Out");
    const ids = (node: MenuNode) =>
      node.kind === "submenu"
        ? node.items.map((i) => (i.kind === "item" ? `${i.id}${i.checked ? "*" : ""}` : ""))
        : [];
    expect(ids(merge)).toEqual(["merge:feature/x"]);
    expect(ids(checkout)).toEqual(["checkout:feature/x", "checkout:main*"]);
  });

  it("caps a long list of branches and says how many are left", () => {
    const branches = Array.from({ length: MAX_MENU_BRANCHES + 5 }, (_, i) => ({
      name: `b${String(i).padStart(3, "0")}`,
      isHead: false,
    }));
    const node = find(buildMenuModel({ ...STATE, branches }), "Check Out");
    expect(node.kind === "submenu" && node.items).toHaveLength(MAX_MENU_BRANCHES + 1);
  });

  it("lists the recent repositories, or says there are none", () => {
    const recent = find(buildMenuModel(STATE), "Open Recent");
    expect(
      recent.kind === "submenu" && recent.items.map((i) => (i.kind === "item" ? i.id : "-")),
    ).toEqual(["recent:/fake/alpha", "-", "clearRecents"]);
    const empty = find(buildMenuModel({ ...STATE, recents: [] }), "Open Recent");
    expect(empty.kind === "submenu" && empty.items).toHaveLength(1);
  });

  it("shows the read-only mode, the density and the activity log as checked entries", () => {
    const state = { ...STATE, readOnly: true, density: "detailed" as const, activityOpen: true };
    const checked = (label: string) => {
      const node = find(buildMenuModel(state), label);
      return node.kind === "item" && node.checked;
    };
    expect(checked("Read-only Mode")).toBe(true);
    expect(checked("Detailed Rows")).toBe(true);
    expect(checked("Compact Rows")).toBe(false);
    expect(checked("Activity Log")).toBe(true);
  });

  it("splits an action into its name and argument", () => {
    expect(parseMenuAction("checkout:feature/x")).toEqual({ name: "checkout", arg: "feature/x" });
    expect(parseMenuAction("recent:/a:b")).toEqual({ name: "recent", arg: "/a:b" });
    expect(parseMenuAction("fetch")).toEqual({ name: "fetch", arg: "" });
  });
});
