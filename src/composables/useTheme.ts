import { ref, watchEffect } from "vue";

export type ThemeMode = "system" | "light" | "dark";

const STORAGE_KEY = "git-hydra:theme";

function readStoredMode(): ThemeMode {
  try {
    const value = localStorage.getItem(STORAGE_KEY);
    if (value === "light" || value === "dark" || value === "system") return value;
  } catch {
    // Storage unavailable: fall back to system.
  }
  return "system";
}

const mode = ref<ThemeMode>(readStoredMode());
const systemQuery = window.matchMedia("(prefers-color-scheme: dark)");
const systemDark = ref(systemQuery.matches);
systemQuery.addEventListener("change", (event) => {
  systemDark.value = event.matches;
});

export function isDark(current: ThemeMode, prefersDark: boolean): boolean {
  return current === "dark" || (current === "system" && prefersDark);
}

watchEffect(() => {
  document.documentElement.classList.toggle("dark", isDark(mode.value, systemDark.value));
});

export function useTheme() {
  function setMode(next: ThemeMode) {
    mode.value = next;
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      // Ignore persistence failures.
    }
  }
  return { mode, setMode };
}
