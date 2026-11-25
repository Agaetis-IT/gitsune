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
    expect(localStorage.getItem("gitsune:theme")).toBe("dark");
  });
});

describe("themes", () => {
  beforeEach(() => {
    vi.resetModules();
    localStorage.clear();
    document.documentElement.classList.remove("dark");
    document.documentElement.removeAttribute("style");
    stubMatchMedia(false);
  });

  it("starts on the default theme in system mode", async () => {
    const { useTheme } = await import("./useTheme");
    const t = useTheme();
    expect(t.theme.value.id).toBe("default");
    expect(t.mode.value).toBe("system");
    expect(t.themes.value.length).toBeGreaterThanOrEqual(6);
  });

  it("puts the colors of the chosen theme and variant on the page", async () => {
    const { useTheme } = await import("./useTheme");
    const { BUILTIN_THEMES } = await import("@/lib/theme/builtin");
    const t = useTheme();
    const nord = BUILTIN_THEMES.find((x) => x.id === "nord")!;
    t.setTheme("nord");
    t.setMode("light");
    await Promise.resolve();
    const root = document.documentElement;
    expect(root.style.getPropertyValue("--background")).toBe(nord.light.colors.background);
    expect(root.style.getPropertyValue("--sidebar")).toBe(nord.light.colors.card);
    expect(root.style.getPropertyValue("--radius")).toBe("0.625rem");
    expect(root.classList.contains("dark")).toBe(false);

    t.setMode("dark");
    await Promise.resolve();
    expect(root.style.getPropertyValue("--background")).toBe(nord.dark.colors.background);
    expect(root.classList.contains("dark")).toBe(true);
  });

  it("gives the graph the lane colors of the variant in use", async () => {
    const { useTheme } = await import("./useTheme");
    const { BUILTIN_THEMES } = await import("@/lib/theme/builtin");
    const t = useTheme();
    const gruvbox = BUILTIN_THEMES.find((x) => x.id === "gruvbox")!;
    t.setTheme("gruvbox");
    t.setMode("dark");
    expect(t.palette.value.lanes).toEqual(gruvbox.dark.lanes);
    t.setMode("light");
    expect(t.palette.value.lanes).toEqual(gruvbox.light.lanes);
    expect(t.palette.value.anonymous).toBe(gruvbox.light.anonymous);
  });

  it("remembers the theme and the mode after a restart", async () => {
    let t = (await import("./useTheme")).useTheme();
    t.setTheme("dracula");
    t.setMode("dark");
    vi.resetModules();
    t = (await import("./useTheme")).useTheme();
    expect(t.themeId.value).toBe("dracula");
    expect(t.mode.value).toBe("dark");
  });

  it("an unknown theme id is ignored, and a stored one that vanished falls back to the default", async () => {
    const { useTheme } = await import("./useTheme");
    const t = useTheme();
    t.setTheme("nope");
    expect(t.themeId.value).toBe("default");
    localStorage.setItem("gitsune:theme-id", "custom-gone");
    vi.resetModules();
    const again = (await import("./useTheme")).useTheme();
    expect(again.theme.value.id).toBe("default");
  });

  it("makes a custom copy, edits it, and never changes a built-in theme", async () => {
    const { useTheme } = await import("./useTheme");
    const { BUILTIN_THEMES } = await import("@/lib/theme/builtin");
    const t = useTheme();
    const copy = t.createFrom("nord");
    expect(copy.builtin).toBe(false);
    expect(copy.name).toBe("Nord copy");
    expect(t.themeId.value).toBe(copy.id);
    expect(t.createFrom("nord").name).toBe("Nord copy 2");

    const changed = structuredClone(copy);
    changed.light.colors.background = "#123456";
    expect(t.saveCustom(changed)).toBe(true);
    expect(t.customThemes.value.find((x) => x.id === copy.id)?.light.colors.background).toBe(
      "#123456",
    );
    expect(BUILTIN_THEMES.find((x) => x.id === "nord")?.light.colors.background).not.toBe(
      "#123456",
    );
    expect(t.saveCustom({ ...changed, id: "nord" })).toBe(false);
    expect(JSON.parse(localStorage.getItem("gitsune:themes") ?? "[]")).toHaveLength(2);
  });

  it("deleting the selected custom theme goes back to the default; built-in ones cannot be deleted", async () => {
    const { useTheme } = await import("./useTheme");
    const t = useTheme();
    const copy = t.createFrom("github");
    t.removeCustom(copy.id);
    expect(t.themeId.value).toBe("default");
    expect(t.customThemes.value).toHaveLength(0);
    t.removeCustom("nord");
    expect(t.themes.value.some((x) => x.id === "nord")).toBe(true);
  });

  it("imports a theme from JSON and says what is wrong with bad text", async () => {
    const { useTheme } = await import("./useTheme");
    const { BUILTIN_THEMES } = await import("@/lib/theme/builtin");
    const { exportTheme } = await import("@/lib/theme/types");
    const t = useTheme();
    const result = t.importTheme(exportTheme(BUILTIN_THEMES[2]));
    expect("theme" in result && result.theme.name).toBe(BUILTIN_THEMES[2].name);
    expect(t.themeId.value).toBe("theme" in result ? result.theme.id : "");
    expect(t.customThemes.value).toHaveLength(1);

    expect(t.importTheme("{nope")).toEqual({ error: "This is not valid JSON." });
    const bad = t.importTheme('{"name":"X"}');
    expect("error" in bad && bad.error).toContain("not a complete theme");
    expect(t.customThemes.value).toHaveLength(1);
  });

  it("ignores broken stored themes", async () => {
    localStorage.setItem("gitsune:themes", '[{"name":"x"}, 5, null]');
    const { useTheme } = await import("./useTheme");
    expect(useTheme().customThemes.value).toEqual([]);
    localStorage.setItem("gitsune:themes", "not json");
    vi.resetModules();
    expect((await import("./useTheme")).useTheme().customThemes.value).toEqual([]);
  });

  it("shows a theme being edited without saving it, and goes back when it stops", async () => {
    const { useTheme } = await import("./useTheme");
    const t = useTheme();
    const copy = t.createFrom("default");
    t.setMode("light");
    const draft = structuredClone(copy);
    draft.light.colors.background = "#abcdef";
    t.preview(draft);
    await Promise.resolve();
    expect(document.documentElement.style.getPropertyValue("--background")).toBe("#abcdef");
    expect(t.customThemes.value[0].light.colors.background).not.toBe("#abcdef");
    t.preview(null);
    await Promise.resolve();
    expect(document.documentElement.style.getPropertyValue("--background")).not.toBe("#abcdef");
  });
});

describe("themes for a light and a dark system", () => {
  beforeEach(() => {
    vi.resetModules();
    localStorage.clear();
    document.documentElement.classList.remove("dark");
    document.documentElement.removeAttribute("style");
  });

  it("uses the theme set for the system in system mode and the chosen one otherwise", async () => {
    stubMatchMedia(true);
    const { useTheme } = await import("./useTheme");
    const t = useTheme();
    t.setTheme("nord");
    t.setSystemTheme("light", "catppuccin-latte");
    t.setSystemTheme("dark", "catppuccin-mocha");
    expect(t.theme.value.id).toBe("catppuccin-mocha");
    t.setMode("light");
    expect(t.theme.value.id).toBe("nord");
    t.setMode("system");
    expect(t.theme.value.id).toBe("catppuccin-mocha");
    t.setSystemTheme("dark", null);
    expect(t.theme.value.id).toBe("nord");
    expect(localStorage.getItem("gitsune:theme-id-dark")).toBeNull();
    expect(localStorage.getItem("gitsune:theme-id-light")).toBe("catppuccin-latte");
  });

  it("ignores an unknown theme and forgets a deleted one", async () => {
    stubMatchMedia(false);
    const { useTheme } = await import("./useTheme");
    const t = useTheme();
    t.setSystemTheme("light", "nope");
    expect(t.lightThemeId.value).toBeNull();
    const copy = t.createFrom("nord");
    t.setSystemTheme("light", copy.id);
    expect(t.theme.value.id).toBe(copy.id);
    t.removeCustom(copy.id);
    expect(t.lightThemeId.value).toBeNull();
    expect(t.theme.value.id).toBe("default");
  });
});

describe("syntax colors and fonts on the page", () => {
  beforeEach(() => {
    vi.resetModules();
    localStorage.clear();
    document.documentElement.removeAttribute("style");
    stubMatchMedia(false);
  });

  it("puts the syntax colors on the page, and the fonts only when the theme sets them", async () => {
    const { useTheme } = await import("./useTheme");
    const t = useTheme();
    await Promise.resolve();
    const root = document.documentElement;
    expect(root.style.getPropertyValue("--syntax-keyword")).toMatch(/^#/);
    expect(root.style.getPropertyValue("--app-font-ui")).toBe("");

    const copy = t.createFrom("nord");
    t.saveCustom({ ...copy, fonts: { ui: "Inter, sans-serif", size: 14 } });
    await Promise.resolve();
    expect(root.style.getPropertyValue("--app-font-ui")).toBe("Inter, sans-serif");
    expect(root.style.getPropertyValue("--app-font-size")).toBe("14px");
    expect(root.style.getPropertyValue("--app-font-code")).toBe("");

    t.setTheme("default");
    await Promise.resolve();
    expect(root.style.getPropertyValue("--app-font-ui")).toBe("");
    expect(root.style.getPropertyValue("--app-font-size")).toBe("");
  });
});
