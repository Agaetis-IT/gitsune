import { computed, ref, watchEffect } from "vue";
import { BUILTIN_THEMES, DEFAULT_THEME_ID } from "@/lib/theme/builtin";
import {
  COLOR_TOKENS,
  duplicateTheme,
  parseTheme,
  type Theme,
  type ThemeMode,
  type ThemeVariant,
} from "@/lib/theme/types";

export type { ThemeMode } from "@/lib/theme/types";

const MODE_KEY = "git-hydra:theme";
const THEME_KEY = "git-hydra:theme-id";
const CUSTOM_KEY = "git-hydra:themes";

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null; // Storage unavailable: use the defaults.
  }
}

function write(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Ignore persistence failures.
  }
}

function readMode(): ThemeMode {
  const value = read(MODE_KEY);
  return value === "light" || value === "dark" || value === "system" ? value : "system";
}

function readCustomThemes(): Theme[] {
  try {
    const parsed: unknown = JSON.parse(read(CUSTOM_KEY) ?? "[]");
    if (!Array.isArray(parsed)) return [];
    return parsed
      .map(parseTheme)
      .filter((theme): theme is Theme => theme !== null && theme.id !== "");
  } catch {
    return [];
  }
}

const mode = ref<ThemeMode>(readMode());
const customThemes = ref<Theme[]>(readCustomThemes());
const themeId = ref<string>(read(THEME_KEY) ?? DEFAULT_THEME_ID);
/** A theme being edited, shown live without being saved. */
const draft = ref<Theme | null>(null);

const systemQuery = window.matchMedia("(prefers-color-scheme: dark)");
const systemDark = ref(systemQuery.matches);
systemQuery.addEventListener("change", (event) => {
  systemDark.value = event.matches;
});

export function isDark(current: ThemeMode, prefersDark: boolean): boolean {
  return current === "dark" || (current === "system" && prefersDark);
}

const allThemes = computed<Theme[]>(() => [...BUILTIN_THEMES, ...customThemes.value]);
/** The chosen theme; the default one when it was deleted or never existed. */
const theme = computed<Theme>(
  () => allThemes.value.find((t) => t.id === themeId.value) ?? BUILTIN_THEMES[0],
);
const active = computed<Theme>(() => draft.value ?? theme.value);
const dark = computed(() => isDark(mode.value, systemDark.value));
const variant = computed<ThemeVariant>(() => (dark.value ? active.value.dark : active.value.light));

/** Tokens the sidebar takes from the others, so a theme does not have to repeat them. */
const SIDEBAR: Array<[string, string]> = [
  ["sidebar", "card"],
  ["sidebar-foreground", "card-foreground"],
  ["sidebar-primary", "primary"],
  ["sidebar-primary-foreground", "primary-foreground"],
  ["sidebar-accent", "accent"],
  ["sidebar-accent-foreground", "accent-foreground"],
  ["sidebar-border", "border"],
  ["sidebar-ring", "ring"],
];

/** Puts a theme variant on the page: the dark class, the CSS variables, the radius. */
export function applyVariant(root: HTMLElement, themeToApply: Theme, isDarkMode: boolean): void {
  const v = isDarkMode ? themeToApply.dark : themeToApply.light;
  root.classList.toggle("dark", isDarkMode);
  for (const token of COLOR_TOKENS) root.style.setProperty(`--${token}`, v.colors[token]);
  for (const [name, from] of SIDEBAR) {
    root.style.setProperty(`--${name}`, v.colors[from as keyof typeof v.colors]);
  }
  root.style.setProperty("--radius", `${themeToApply.radius}rem`);
}

watchEffect(() => applyVariant(document.documentElement, active.value, dark.value));

let counter = 0;
const newId = () => `custom-${Date.now().toString(36)}-${++counter}`;

function persistCustom(): void {
  write(CUSTOM_KEY, JSON.stringify(customThemes.value));
}

export function useTheme() {
  function setMode(next: ThemeMode) {
    mode.value = next;
    write(MODE_KEY, next);
  }

  function setTheme(id: string) {
    if (!allThemes.value.some((t) => t.id === id)) return;
    themeId.value = id;
    write(THEME_KEY, id);
  }

  /** A new custom theme made from an existing one, selected right away. */
  function createFrom(id: string): Theme {
    const source = allThemes.value.find((t) => t.id === id) ?? BUILTIN_THEMES[0];
    const taken = new Set(allThemes.value.map((t) => t.name));
    let name = `${source.name} copy`;
    for (let n = 2; taken.has(name); n++) name = `${source.name} copy ${n}`;
    const copy = duplicateTheme(source, newId(), name);
    customThemes.value = [...customThemes.value, copy];
    persistCustom();
    setTheme(copy.id);
    return copy;
  }

  /** Saves the changes to a custom theme (built-in themes cannot be changed). */
  function saveCustom(changed: Theme): boolean {
    const index = customThemes.value.findIndex((t) => t.id === changed.id);
    if (index === -1) return false;
    const next = [...customThemes.value];
    next[index] = { ...changed, builtin: false };
    customThemes.value = next;
    persistCustom();
    return true;
  }

  function removeCustom(id: string) {
    if (!customThemes.value.some((t) => t.id === id)) return;
    customThemes.value = customThemes.value.filter((t) => t.id !== id);
    persistCustom();
    if (themeId.value === id) setTheme(DEFAULT_THEME_ID);
  }

  /** Adds a theme from JSON text. Returns the theme, or what is wrong with the text. */
  function importTheme(text: string): { theme: Theme } | { error: string } {
    let json: unknown;
    try {
      json = JSON.parse(text);
    } catch {
      return { error: "This is not valid JSON." };
    }
    const parsed = parseTheme(json);
    if (!parsed) {
      return {
        error:
          "This is not a complete theme: it needs a name and, for light or dark, every color of the interface and twelve lane colors, as #rrggbb.",
      };
    }
    const imported = { ...parsed, id: newId() };
    customThemes.value = [...customThemes.value, imported];
    persistCustom();
    setTheme(imported.id);
    return { theme: imported };
  }

  /** Shows a theme being edited on the page without saving it; `null` goes back. */
  function preview(next: Theme | null) {
    draft.value = next;
  }

  return {
    mode,
    setMode,
    themes: allThemes,
    themeId,
    theme,
    setTheme,
    customThemes,
    createFrom,
    saveCustom,
    removeCustom,
    importTheme,
    preview,
    dark,
    /** The colors in use, for the graph. */
    palette: computed(() => ({ lanes: variant.value.lanes, anonymous: variant.value.anonymous })),
  };
}
