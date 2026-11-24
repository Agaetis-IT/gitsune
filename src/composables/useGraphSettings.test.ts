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
    expect(localStorage.getItem("git-hydra:graph-density")).toBe("detailed");
    expect(localStorage.getItem("git-hydra:graph-initials")).toBe("off");
  });

  it("ignores a stored value it does not know", async () => {
    localStorage.setItem("git-hydra:graph-density", "huge");
    const { useGraphSettings } = await import("./useGraphSettings");
    expect(useGraphSettings().density.value).toBe("compact");
  });
});
