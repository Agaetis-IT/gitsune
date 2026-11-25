import { beforeEach, describe, expect, it, vi } from "vitest";

describe("graph settings", () => {
  beforeEach(() => {
    vi.resetModules();
    localStorage.clear();
  });

  it("starts compact with the initials shown", async () => {
    const { useGraphSettings } = await import("./useGraphSettings");
    const s = useGraphSettings();
    expect(s.density.value).toBe("compact");
    expect(s.showInitials.value).toBe(true);
  });

  it("remembers the density and the initials after a restart", async () => {
    let s = (await import("./useGraphSettings")).useGraphSettings();
    s.setDensity("detailed");
    s.setShowInitials(false);
    vi.resetModules();
    s = (await import("./useGraphSettings")).useGraphSettings();
    expect(s.density.value).toBe("detailed");
    expect(s.showInitials.value).toBe(false);
    expect(localStorage.getItem("gitsune:graph-density")).toBe("detailed");
    expect(localStorage.getItem("gitsune:graph-initials")).toBe("off");
  });

  it("ignores a stored value it does not know", async () => {
    localStorage.setItem("gitsune:graph-density", "huge");
    const { useGraphSettings } = await import("./useGraphSettings");
    expect(useGraphSettings().density.value).toBe("compact");
  });
});

describe("graph layout settings", () => {
  beforeEach(() => {
    vi.resetModules();
    localStorage.clear();
  });

  it("shows the author and the date, not the id, until told otherwise", async () => {
    const s = (await import("./useGraphSettings")).useGraphSettings();
    expect(s.columns.value).toEqual({ author: true, date: true, sha: false });
    s.setColumn("sha", true);
    s.setColumn("author", false);
    const again = (await import("./useGraphSettings")).useGraphSettings();
    expect(again.columns.value).toEqual({ author: false, date: true, sha: true });
  });

  it("keeps the column widths within limits and remembers them", async () => {
    const { useGraphSettings, REFS_WIDTH } = await import("./useGraphSettings");
    const s = useGraphSettings();
    expect(s.refsWidth.value).toBe(REFS_WIDTH.initial);
    s.setRefsWidth(5000);
    expect(s.refsWidth.value).toBe(REFS_WIDTH.max);
    s.setRefsWidth(10);
    expect(s.refsWidth.value).toBe(REFS_WIDTH.min);
    s.setGraphWidth(300);
    expect(localStorage.getItem("gitsune:graph-width")).toBe("300");
    s.setGraphWidth(null);
    expect(s.graphWidth.value).toBeNull();
    expect(localStorage.getItem("gitsune:graph-width")).toBeNull();
  });

  it("gives each remote an icon: its choice, or a default", async () => {
    const s = (await import("./useGraphSettings")).useGraphSettings();
    expect(s.remoteIcon("origin")).toBe("users");
    expect(s.remoteIcon("upstream")).toBe("git-fork");
    expect(s.remoteIcon("backup")).toBe("server");
    s.setRemoteIcon("backup", "cloud");
    expect(s.remoteIcon("backup")).toBe("cloud");
    const again = (await import("./useGraphSettings")).useGraphSettings();
    expect(again.remoteIcon("backup")).toBe("cloud");
    again.setRemoteIcon("backup", null);
    expect(again.remoteIcon("backup")).toBe("server");
  });

  it("ignores stored icons it does not know", async () => {
    localStorage.setItem("gitsune:remote-icons", JSON.stringify({ origin: "nope", up: "cloud" }));
    const s = (await import("./useGraphSettings")).useGraphSettings();
    expect(s.remoteIcon("origin")).toBe("users");
    expect(s.remoteIcon("up")).toBe("cloud");
  });
});
