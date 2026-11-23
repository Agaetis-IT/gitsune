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

export interface ThemeVariant {
  colors: Record<ColorToken, string>;
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
  light: ThemeVariant;
  dark: ThemeVariant;
}

/** The setting that picks a variant: follow the system, or force one. */
export type ThemeMode = "system" | "light" | "dark";

const HEX = /^#(?:[0-9a-f]{6}|[0-9a-f]{8})$/i;

/** `#rrggbb` or `#rrggbbaa`. */
export const isColor = (value: unknown): value is string =>
  typeof value === "string" && HEX.test(value);

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
    light: cloneVariant((light ?? dark) as ThemeVariant),
    dark: cloneVariant((dark ?? light) as ThemeVariant),
  };
}

/** A deep copy that also works on a reactive object (`structuredClone` refuses a proxy). */
export const cloneTheme = (theme: Theme): Theme => JSON.parse(JSON.stringify(theme)) as Theme;

/** A copy that can be edited: new id and name, never built in. */
export function duplicateTheme(theme: Theme, id: string, name: string): Theme {
  return {
    id,
    name,
    builtin: false,
    radius: theme.radius,
    light: cloneVariant(theme.light),
    dark: cloneVariant(theme.dark),
  };
}

/** The JSON text of a theme, for export (without the identity of this installation). */
export function exportTheme(theme: Theme): string {
  const { name, radius, light, dark } = theme;
  return JSON.stringify({ name, radius, light, dark }, null, 2);
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
