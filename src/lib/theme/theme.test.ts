import { reactive } from "vue";
import { describe, expect, it } from "vitest";
import { BUILTIN_THEMES, DEFAULT_THEME_ID } from "./builtin";
import {
  cloneTheme,
  COLOR_TOKENS,
  contrastRatio,
  deriveSyntax,
  duplicateTheme,
  exportTheme,
  isColor,
  LANE_COLORS,
  parseTheme,
  splitAlpha,
  SYNTAX_TOKENS,
  syntaxOf,
} from "./types";

describe("built-in themes", () => {
  it("has the default theme first, with distinct ids and names", () => {
    expect(BUILTIN_THEMES[0].id).toBe(DEFAULT_THEME_ID);
    expect(new Set(BUILTIN_THEMES.map((t) => t.id)).size).toBe(BUILTIN_THEMES.length);
    expect(new Set(BUILTIN_THEMES.map((t) => t.name)).size).toBe(BUILTIN_THEMES.length);
    expect(BUILTIN_THEMES.every((t) => t.builtin)).toBe(true);
  });

  for (const theme of BUILTIN_THEMES) {
    for (const mode of ["light", "dark"] as const) {
      const v = theme[mode];
      const label = `${theme.name} (${mode})`;

      it(`${label} sets every token and lane with valid colors`, () => {
        for (const token of COLOR_TOKENS) expect(isColor(v.colors[token]), token).toBe(true);
        expect(v.lanes).toHaveLength(LANE_COLORS);
        expect(v.lanes.every(isColor)).toBe(true);
        expect(isColor(v.anonymous)).toBe(true);
      });

      it(`${label} keeps text readable`, () => {
        const c = v.colors;
        expect(contrastRatio(c.foreground, c.background), "text").toBeGreaterThanOrEqual(4.5);
        expect(contrastRatio(c["card-foreground"], c.card), "panel text").toBeGreaterThanOrEqual(
          4.5,
        );
        expect(
          contrastRatio(c["muted-foreground"], c.background),
          "muted text",
        ).toBeGreaterThanOrEqual(3);
        expect(contrastRatio(c["primary-foreground"], c.primary), "buttons").toBeGreaterThanOrEqual(
          3,
        );
      });

      it(`${label} has twelve different lane colors`, () => {
        expect(new Set(v.lanes.map((l) => l.toLowerCase())).size).toBe(LANE_COLORS);
      });

      it(`${label} draws lanes that stand out from the background`, () => {
        for (const lane of v.lanes) {
          expect(contrastRatio(lane, v.colors.background), lane).toBeGreaterThanOrEqual(1.8);
        }
      });
    }
  }
});

describe("reading a theme from JSON", () => {
  const source = BUILTIN_THEMES[1];
  const json = () => JSON.parse(exportTheme(source));

  it("round-trips an exported theme, as a custom one", () => {
    const parsed = parseTheme(json());
    expect(parsed).not.toBeNull();
    expect(parsed?.name).toBe(source.name);
    expect(parsed?.builtin).toBe(false);
    expect(parsed?.light).toEqual(source.light);
    expect(parsed?.dark).toEqual(source.dark);
    expect(parsed?.radius).toBe(source.radius);
  });

  it("does not export the identity of the installation", () => {
    expect(Object.keys(json()).sort()).toEqual(["dark", "light", "name", "radius"]);
  });

  it("a theme with one variant looks the same in both modes", () => {
    const onlyLight = json();
    delete onlyLight.dark;
    const parsed = parseTheme(onlyLight);
    expect(parsed?.dark).toEqual(parsed?.light);
  });

  it.each([
    ["not an object", "x"],
    ["no name", { ...json(), name: "  " }],
    ["no variant", { name: "Empty" }],
    [
      "a color that is not hex",
      (() => {
        const j = json();
        j.light.colors.background = "red";
        j.dark.colors.background = "red";
        return j;
      })(),
    ],
    [
      "a missing token",
      (() => {
        const j = json();
        delete j.light.colors.border;
        delete j.dark.colors.border;
        return j;
      })(),
    ],
    [
      "too few lanes",
      (() => {
        const j = json();
        j.light.lanes = j.light.lanes.slice(1);
        j.dark.lanes = j.dark.lanes.slice(1);
        return j;
      })(),
    ],
  ])("refuses %s", (_label, value) => {
    expect(parseTheme(value)).toBeNull();
  });

  it("falls back to the default radius when it is out of range, and keeps a valid broken variant out", () => {
    expect(parseTheme({ ...json(), radius: 99 })?.radius).toBe(0.625);
    const j = json();
    j.dark.colors.background = "nope";
    const parsed = parseTheme(j);
    expect(parsed?.dark).toEqual(parsed?.light);
  });

  it("duplicating makes an independent editable copy", () => {
    const copy = duplicateTheme(source, "custom-1", "Mine");
    expect(copy).toMatchObject({ id: "custom-1", name: "Mine", builtin: false });
    copy.light.colors.background = "#123456";
    copy.light.lanes[0] = "#000000";
    expect(source.light.colors.background).not.toBe("#123456");
    expect(source.light.lanes[0]).not.toBe("#000000");
  });
});

describe("cloning for the editor", () => {
  it("copies a reactive theme (what the editor receives) without sharing anything", () => {
    const shown = reactive(BUILTIN_THEMES[1]);
    const copy = cloneTheme(shown);
    expect(copy).toEqual(BUILTIN_THEMES[1]);
    copy.dark.colors.background = "#010203";
    copy.dark.lanes[0] = "#040506";
    expect(BUILTIN_THEMES[1].dark.colors.background).not.toBe("#010203");
    expect(BUILTIN_THEMES[1].dark.lanes[0]).not.toBe("#040506");
  });
});

describe("color helpers", () => {
  it("measures contrast like WCAG", () => {
    expect(contrastRatio("#000000", "#ffffff")).toBeCloseTo(21, 0);
    expect(contrastRatio("#777777", "#777777")).toBeCloseTo(1, 5);
    expect(contrastRatio("#767676", "#ffffff")).toBeGreaterThan(4.5);
  });

  it("accepts #rrggbb and #rrggbbaa only", () => {
    expect(isColor("#aabbcc")).toBe(true);
    expect(isColor("#AABBCC80")).toBe(true);
    expect(isColor("#abc")).toBe(false);
    expect(isColor("rgb(0,0,0)")).toBe(false);
    expect(isColor(12)).toBe(false);
  });

  it("splits the alpha a color picker cannot hold", () => {
    expect(splitAlpha("#ffffff1a")).toEqual({ rgb: "#ffffff", alpha: "1a" });
    expect(splitAlpha("#112233")).toEqual({ rgb: "#112233", alpha: "" });
  });
});

describe("syntax colors", () => {
  const base = BUILTIN_THEMES[0];

  it("are drawn from the lane colors when a theme sets none", () => {
    for (const theme of BUILTIN_THEMES) {
      for (const mode of ["light", "dark"] as const) {
        const colors = syntaxOf(theme[mode]);
        for (const token of SYNTAX_TOKENS) expect(isColor(colors[token]), token).toBe(true);
        // Comments use the muted text color.
        expect(colors.comment).toBe(theme[mode].colors["muted-foreground"].slice(0, 7));
      }
    }
  });

  it("pick the lane whose hue is the nearest to the usual one", () => {
    const variant = cloneTheme(base).dark;
    variant.lanes = variant.lanes.map(() => "#808080");
    variant.lanes[0] = "#a050e0"; // purple
    variant.lanes[1] = "#40c050"; // green
    variant.lanes[2] = "#e08030"; // orange
    variant.lanes[3] = "#3080e0"; // blue
    const colors = deriveSyntax(variant);
    expect(colors.keyword).toBe("#a050e0");
    expect(colors.string).toBe("#40c050");
    expect(colors.number).toBe("#e08030");
    expect(colors.function).toBe("#3080e0");
  });

  it("use the text color when a theme has only grays", () => {
    const variant = cloneTheme(base).light;
    variant.lanes = variant.lanes.map(() => "#777777");
    variant.anonymous = "#777777";
    expect(deriveSyntax(variant).keyword).toBe(variant.colors.foreground.slice(0, 7));
  });

  it("are kept by a theme that sets them, through copy, export and import", () => {
    const theme = cloneTheme(base);
    theme.dark.syntax = {
      keyword: "#111111",
      string: "#222222",
      number: "#333333",
      function: "#444444",
      type: "#555555",
      comment: "#666666",
    };
    expect(syntaxOf(theme.dark).keyword).toBe("#111111");
    const copy = duplicateTheme(theme, "x", "Copy");
    expect(copy.dark.syntax?.string).toBe("#222222");
    const parsed = parseTheme(JSON.parse(exportTheme(theme)));
    expect(parsed?.dark.syntax?.comment).toBe("#666666");
    expect(parsed?.light.syntax).toBeUndefined();
  });

  it("an incomplete set of syntax colors is ignored, not an error", () => {
    const raw = JSON.parse(exportTheme(base));
    raw.dark.syntax = { keyword: "#111111" };
    const parsed = parseTheme(raw);
    expect(parsed).not.toBeNull();
    expect(parsed?.dark.syntax).toBeUndefined();
  });
});

describe("theme fonts", () => {
  const withFonts = (fonts: unknown) => {
    const raw = JSON.parse(exportTheme(BUILTIN_THEMES[0]));
    raw.fonts = fonts;
    return parseTheme(raw);
  };

  it("are read from an imported theme and exported again", () => {
    const parsed = withFonts({
      ui: "Inter, sans-serif",
      code: "'JetBrains Mono', monospace",
      size: 14,
    });
    expect(parsed?.fonts).toEqual({
      ui: "Inter, sans-serif",
      code: "'JetBrains Mono', monospace",
      size: 14,
    });
    expect(JSON.parse(exportTheme(parsed!)).fonts.size).toBe(14);
  });

  it("refuse anything that could break out of a CSS value", () => {
    expect(withFonts({ ui: "Inter; background: red" })?.fonts).toBeUndefined();
    expect(withFonts({ code: "x} body{display:none" })?.fonts).toBeUndefined();
    expect(withFonts({ ui: "url(http://evil/x)" })?.fonts).toBeUndefined();
    expect(withFonts({ ui: "a".repeat(300) })?.fonts).toBeUndefined();
  });

  it("keep the size within limits and leave an old theme without fonts", () => {
    expect(withFonts({ size: 40 })?.fonts).toBeUndefined();
    expect(withFonts({ size: 3 })?.fonts).toBeUndefined();
    expect(withFonts({ size: 13.6 })?.fonts?.size).toBe(14);
    expect(parseTheme(JSON.parse(exportTheme(BUILTIN_THEMES[0])))?.fonts).toBeUndefined();
  });
});
