import { beforeEach, describe, expect, it, vi } from "vitest";

function stubMatchMedia(matches: boolean) {
  vi.stubGlobal("matchMedia", () => ({
    matches,
    addEventListener: () => {},
    removeEventListener: () => {},
  }));
}

describe("useTheme", () => {
  beforeEach(() => {
    vi.resetModules();
    localStorage.clear();
    document.documentElement.classList.remove("dark");
  });

  it("isDark follows the system only in system mode", async () => {
    stubMatchMedia(false);
    const { isDark } = await import("./useTheme");
    expect(isDark("system", true)).toBe(true);
    expect(isDark("system", false)).toBe(false);
    expect(isDark("dark", false)).toBe(true);
    expect(isDark("light", true)).toBe(false);
  });

  it("applies the dark class and persists the chosen mode", async () => {
    stubMatchMedia(false);
    const { useTheme } = await import("./useTheme");
    const { setMode } = useTheme();
    setMode("dark");
    await Promise.resolve();
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    expect(localStorage.getItem("git-hydra:theme")).toBe("dark");
  });
});
