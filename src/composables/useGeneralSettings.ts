import { ref } from "vue";

const RESTORE_KEY = "git-hydra:restore-tabs";

function readRestore(): boolean {
  try {
    return localStorage.getItem(RESTORE_KEY) !== "off";
  } catch {
    return true; // Storage unavailable: the default.
  }
}

/** Reopen the tabs of the last session at start (on unless switched off). */
const restoreTabs = ref(readRestore());

export function useGeneralSettings() {
  function setRestoreTabs(value: boolean) {
    restoreTabs.value = value;
    try {
      localStorage.setItem(RESTORE_KEY, value ? "on" : "off");
    } catch {
      // Not kept for the next session.
    }
  }
  return { restoreTabs, setRestoreTabs };
}
