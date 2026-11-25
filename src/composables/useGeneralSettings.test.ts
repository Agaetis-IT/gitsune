import { beforeEach, describe, expect, it, vi } from "vitest";

describe("general settings", () => {
  beforeEach(() => {
    vi.resetModules();
    localStorage.clear();
  });

  it("restores the tabs unless switched off, and remembers the choice", async () => {
    let s = (await import("./useGeneralSettings")).useGeneralSettings();
    expect(s.restoreTabs.value).toBe(true);
    s.setRestoreTabs(false);
    vi.resetModules();
    s = (await import("./useGeneralSettings")).useGeneralSettings();
    expect(s.restoreTabs.value).toBe(false);
    expect(localStorage.getItem("gitsune:restore-tabs")).toBe("off");
  });
});
