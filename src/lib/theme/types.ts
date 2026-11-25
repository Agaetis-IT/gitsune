/**
 * A theme is plain data: colors for the interface and for the lanes of the graph, in a light
 * and a dark variant, and a corner radius. Nothing here touches the page; see `apply.ts`.
 */

/** The interface colors a theme sets; each is a shadcn token (`--background`...). */
export const COLOR_TOKENS = [
  "background",
  "foreground",
  "card",
  "card-foreground",
  "popover",
  "popover-foreground",
  "primary",
  "primary-foreground",
  "secondary",
  "secondary-foreground",
  "muted",
  "muted-foreground",
  "accent",
  "accent-foreground",
  "destructive",
  "border",
  "input",
  "ring",
] as const;

export type ColorToken = (typeof COLOR_TOKENS)[number];

/** Must match `PALETTE_SIZE` in `src-tauri/src/layout/naming.rs`. */
export const LANE_COLORS = 12;

/** What the editor shows for each token. */
export const TOKEN_LABELS: Record<ColorToken, string> = {
  background: "Background",
  foreground: "Text",
  card: "Panels",
  "card-foreground": "Panel text",
  popover: "Menus and dialogs",
  "popover-foreground": "Menu text",
  primary: "Primary (buttons)",
  "primary-foreground": "Primary text",
  secondary: "Secondary",
  "secondary-foreground": "Secondary text",
  muted: "Muted background",
  "muted-foreground": "Muted text",
  accent: "Hover and selection",
  "accent-foreground": "Hover text",
  destructive: "Destructive",
  border: "Borders",
  input: "Input borders",
  ring: "Focus ring",
};

/** The colors of the syntax highlighting in the diffs. */
export const SYNTAX_TOKENS = [
  "keyword",
  "string",
  "number",
  "function",
  "type",
  "comment",
] as const;

export type SyntaxToken = (typeof SYNTAX_TOKENS)[number];

export type SyntaxColors = Record<SyntaxToken, string>;

export const SYNTAX_LABELS: Record<SyntaxToken, string> = {
  keyword: "Keywords",
  string: "Strings",
  number: "Numbers",
  function: "Functions and names",
  type: "Types and properties",
  comment: "Comments",
};

/** Fonts and size of the interface; what is left out is the default of the application. */
export interface ThemeFonts {
  /** The font of the interface (a CSS font-family list). */
  ui?: string;
  /** The font of code and diffs. */
  code?: string;
  /** The base size in pixels: everything is sized from it. */
  size?: number;
}

export const FONT_SIZE = { min: 12, max: 20, default: 16 } as const;

export interface ThemeVariant {
  colors: Record<ColorToken, string>;
  /**
   * Colors of the syntax highlighting. A theme without them (an older one) gets colors drawn
   * from its lane colors, see `syntaxOf`.
   */
  syntax?: SyntaxColors;
  /** Colors of the branch lines of the graph, by palette index. */
  lanes: string[];
  /** Color of lines that belong to no named branch. */
  anonymous: string;
}

export interface Theme {
  id: string;
  name: string;
  /** Provided with the app: it cannot be edited or deleted, only duplicated. */
  builtin: boolean;
  /** Corner radius, in rem. */
  radius: number;
  fonts?: ThemeFonts;
  light: ThemeVariant;
  dark: ThemeVariant;
}

/** The setting that picks a variant: follow the system, or force one. */
export type ThemeMode = "system" | "light" | "dark";

const HEX = /^#(?:[0-9a-f]{6}|[0-9a-f]{8})$/i;

/** `#rrggbb` or `#rrggbbaa`. */
export const isColor = (value: unknown): value is string =>
  typeof value === "string" && HEX.test(value);

const isSyntax = (value: unknown): value is SyntaxColors =>
  typeof value === "object" &&
  value !== null &&
  SYNTAX_TOKENS.every((token) => isColor((value as SyntaxColors)[token]));

/** A font list is plain names: nothing that could close a CSS rule or load something. */
const FONT_LIST = /^[\p{L}\p{N} ,'"._-]{1,120}$/u;

/** Whether a font list is acceptable (see `FONT_LIST`). */
export const isFontList = (value: string): boolean => FONT_LIST.test(value.trim());

const parseFonts = (value: unknown): ThemeFonts | undefined => {
  if (typeof value !== "object" || value === null) return undefined;
  const raw = value as Record<string, unknown>;
  const fonts: ThemeFonts = {};
  if (typeof raw.ui === "string" && FONT_LIST.test(raw.ui.trim())) fonts.ui = raw.ui.trim();
  if (typeof raw.code === "string" && FONT_LIST.test(raw.code.trim())) fonts.code = raw.code.trim();
  if (typeof raw.size === "number" && raw.size >= FONT_SIZE.min && raw.size <= FONT_SIZE.max) {
    fonts.size = Math.round(raw.size);
  }
  return Object.keys(fonts).length > 0 ? fonts : undefined;
};

const isVariant = (value: unknown): value is ThemeVariant => {
  if (typeof value !== "object" || value === null) return false;
  const v = value as ThemeVariant;
  return (
    typeof v.colors === "object" &&
    v.colors !== null &&
    COLOR_TOKENS.every((token) => isColor(v.colors[token])) &&
    Array.isArray(v.lanes) &&
    v.lanes.length === LANE_COLORS &&
    v.lanes.every(isColor) &&
    isColor(v.anonymous)
  );
};

const cloneVariant = (variant: ThemeVariant): ThemeVariant => ({
  colors: { ...variant.colors },
  lanes: [...variant.lanes],
  anonymous: variant.anonymous,
  ...(isSyntax(variant.syntax) ? { syntax: { ...variant.syntax } } : {}),
});

/**
 * Reads a theme from JSON (an import, or what was stored). A theme with one variant only is
 * accepted: the missing one reuses it, so the theme looks the same in both modes. Anything
 * else that is not a complete theme gives `null`.
 */
export function parseTheme(value: unknown): Theme | null {
  if (typeof value !== "object" || value === null) return null;
  const raw = value as Partial<Theme> & { id?: unknown; name?: unknown };
  const name = typeof raw.name === "string" ? raw.name.trim() : "";
  if (!name) return null;
  const light = isVariant(raw.light) ? raw.light : null;
  const dark = isVariant(raw.dark) ? raw.dark : null;
  if (!light && !dark) return null;
  const radius =
    typeof raw.radius === "number" && raw.radius >= 0 && raw.radius <= 2 ? raw.radius : 0.625;
  return {
    id: typeof raw.id === "string" && raw.id ? raw.id : "",
    name: name.slice(0, 60),
    builtin: false,
    radius,
    ...fontsField(parseFonts(raw.fonts)),
    light: cloneVariant((light ?? dark) as ThemeVariant),
    dark: cloneVariant((dark ?? light) as ThemeVariant),
  };
}

const fontsField = (fonts: ThemeFonts | undefined) => (fonts ? { fonts: { ...fonts } } : {});

/** A deep copy that also works on a reactive object (`structuredClone` refuses a proxy). */
export const cloneTheme = (theme: Theme): Theme => JSON.parse(JSON.stringify(theme)) as Theme;

/** A copy that can be edited: new id and name, never built in. */
export function duplicateTheme(theme: Theme, id: string, name: string): Theme {
  return {
    id,
    name,
    builtin: false,
    radius: theme.radius,
    ...fontsField(theme.fonts),
    light: cloneVariant(theme.light),
    dark: cloneVariant(theme.dark),
  };
}

/** The JSON text of a theme, for export (without the identity of this installation). */
export function exportTheme(theme: Theme): string {
  const { name, radius, fonts, light, dark } = theme;
  return JSON.stringify({ name, radius, fonts, light, dark }, null, 2);
}

// ---------- colors ----------

const channel = (hex: string, from: number) => Number.parseInt(hex.slice(from, from + 2), 16);

/** Relative luminance of a `#rrggbb[aa]` color (WCAG), ignoring its alpha. */
export function luminance(hex: string): number {
  const linear = (c: number) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return (
    0.2126 * linear(channel(hex, 1)) +
    0.7152 * linear(channel(hex, 3)) +
    0.0722 * linear(channel(hex, 5))
  );
}

/** WCAG contrast ratio between two colors, from 1 to 21. */
export function contrastRatio(a: string, b: string): number {
  const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (light + 0.05) / (dark + 0.05);
}

/** Splits `#rrggbbaa` into the `#rrggbb` a color picker knows and the alpha suffix to keep. */
export function splitAlpha(hex: string): { rgb: string; alpha: string } {
  return { rgb: hex.slice(0, 7), alpha: hex.length === 9 ? hex.slice(7) : "" };
}

// ---------- syntax colors ----------

/** Hue of a `#rrggbb[aa]` color in degrees (0 to 360), and how colorful it is (0 to 1). */
export function hueOf(hex: string): { hue: number; chroma: number } {
  const [r, g, b] = [channel(hex, 1), channel(hex, 3), channel(hex, 5)].map((c) => c / 255);
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const delta = max - min;
  if (delta === 0) return { hue: 0, chroma: 0 };
  let hue: number;
  if (max === r) hue = ((g - b) / delta) % 6;
  else if (max === g) hue = (b - r) / delta + 2;
  else hue = (r - g) / delta + 4;
  return { hue: (hue * 60 + 360) % 360, chroma: delta };
}

/** The hue each kind of token is drawn in when a theme does not set its syntax colors. */
const SYNTAX_HUES: Record<Exclude<SyntaxToken, "comment">, number> = {
  keyword: 285,
  string: 140,
  number: 30,
  function: 225,
  type: 185,
};

const hueDistance = (a: number, b: number) => {
  const d = Math.abs(a - b) % 360;
  return d > 180 ? 360 - d : d;
};

/**
 * Syntax colors for a theme that has none: for each kind of token the lane color whose hue is
 * the closest to the usual one (purple keywords, green strings...), and the muted text color
 * for comments. Colors without any hue (grays) are not considered.
 */
export function deriveSyntax(variant: ThemeVariant): SyntaxColors {
  const candidates = [...variant.lanes, variant.anonymous]
    .map((color) => ({ color: color.slice(0, 7), ...hueOf(color) }))
    .filter((c) => c.chroma > 0.15);
  const pick = (token: Exclude<SyntaxToken, "comment">): string => {
    const best = candidates
      .map((c) => ({ c, d: hueDistance(c.hue, SYNTAX_HUES[token]) }))
      .sort((a, b) => a.d - b.d)[0];
    return best ? best.c.color : variant.colors.foreground.slice(0, 7);
  };
  return {
    keyword: pick("keyword"),
    string: pick("string"),
    number: pick("number"),
    function: pick("function"),
    type: pick("type"),
    comment: variant.colors["muted-foreground"].slice(0, 7),
  };
}

/** The syntax colors in use for a variant: its own, or the ones drawn from its lanes. */
export const syntaxOf = (variant: ThemeVariant): SyntaxColors =>
  isSyntax(variant.syntax) ? variant.syntax : deriveSyntax(variant);
