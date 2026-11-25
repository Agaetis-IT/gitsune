import { describe, expect, it } from "vitest";
import { buildMenuModel, type AppMenuState } from "./appMenu";
import {
  formatAccelerator,
  fuzzyScore,
  paletteEntries,
  rankEntries,
  remember,
  type PaletteEntry,
} from "./commandPalette";

const STATE: AppMenuState = {
  hasRepo: true,
  canWrite: true,
  remoteBusy: false,
  readOnly: false,
  detached: false,
  hasHead: true,
  tabCount: 1,
  recents: [{ path: "/fake/alpha", name: "alpha" }],
  branches: [
    { name: "main", isHead: true },
    { name: "feature/x", isHead: false },
  ],
  density: "compact",
  activityOpen: false,
  historyOpen: false,
  editorLabel: "Zed",
  fileManager: "Finder",
  terminalLabel: "Terminal",
};

describe("paletteEntries", () => {
  it("lists the entries of the menu with their path, branches and recents included", () => {
    const entries = paletteEntries(buildMenuModel(STATE));
    const paths = entries.map((e) => e.path);
    expect(paths).toContain("Repository › Fetch");
    expect(paths).toContain("Branch › New Branch…");
    expect(paths).toContain("Branch › Check Out › feature/x");
    expect(paths).toContain("File › Open Recent › alpha");
    expect(entries.find((e) => e.path === "Repository › Fetch")).toMatchObject({
      id: "fetch",
      accelerator: "CmdOrCtrl+Shift+F",
    });
  });

  it("leaves out greyed entries, predefined ones and the Window menu", () => {
    const entries = paletteEntries(buildMenuModel({ ...STATE, hasRepo: false }));
    const paths = entries.map((e) => e.path);
    expect(paths).not.toContain("Repository › Fetch");
    expect(paths).toContain("File › Open a Repository…");
    expect(paths.some((p) => p.startsWith("Window"))).toBe(false);
    expect(paths.some((p) => /Quit|Minimize|Copy/.test(p))).toBe(false);
  });

  it("hides write actions in read-only mode", () => {
    const paths = paletteEntries(buildMenuModel({ ...STATE, canWrite: false })).map((e) => e.path);
    expect(paths).not.toContain("Branch › New Branch…");
    expect(paths).toContain("Repository › Fetch");
  });
});

describe("fuzzyScore", () => {
  it("needs the letters in order, and ignores spaces and case", () => {
    expect(fuzzyScore("ck fe", "Branch › Check Out › feature/x")).not.toBeNull();
    expect(fuzzyScore("xf", "feature/x")).toBeNull();
    expect(fuzzyScore("", "anything")).toBe(0);
  });

  it("prefers word starts and letters that follow each other", () => {
    const tight = fuzzyScore("push", "Repository › Push") ?? 0;
    const loose = fuzzyScore("push", "Pull the upstream, then show history") ?? 0;
    expect(tight).toBeGreaterThan(loose);
  });
});

describe("rankEntries", () => {
  const entries: PaletteEntry[] = [
    { id: "fetch", path: "Repository › Fetch" },
    { id: "push", path: "Repository › Push" },
    { id: "pull", path: "Repository › Pull" },
  ];

  it("puts the matches first, best first, and drops the rest", () => {
    expect(rankEntries(entries, "pu").map((e) => e.id)).toEqual(["push", "pull"]);
    expect(rankEntries(entries, "zzz")).toEqual([]);
  });

  it("starts with the entries run lately when there is no query", () => {
    expect(rankEntries(entries, "", ["pull", "fetch"]).map((e) => e.id)).toEqual([
      "pull",
      "fetch",
      "push",
    ]);
    expect(rankEntries(entries, "  ").map((e) => e.id)).toEqual(["fetch", "push", "pull"]);
  });
});

describe("formatAccelerator and remember", () => {
  it("writes shortcuts the way each system does", () => {
    expect(formatAccelerator("CmdOrCtrl+Shift+F", true)).toBe("⌘⇧F");
    expect(formatAccelerator("CmdOrCtrl+Shift+F", false)).toBe("Ctrl+Shift+F");
    expect(formatAccelerator("CmdOrCtrl+Alt+Z", true)).toBe("⌘⌥Z");
  });

  it("keeps the last five distinct ids, newest first", () => {
    let recent: string[] = [];
    for (const id of ["a", "b", "c", "a", "d", "e", "f"]) recent = remember(recent, id);
    expect(recent).toEqual(["f", "e", "d", "a", "c"]);
  });
});
