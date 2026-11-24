import { ref } from "vue";
import type { Density } from "@/lib/metro/metrics";

const INITIALS_KEY = "git-hydra:graph-initials";
const DENSITY_KEY = "git-hydra:graph-density";

function readInitials(): boolean {
  try {
    return localStorage.getItem(INITIALS_KEY) !== "off";
  } catch {
    return true; // Storage unavailable: the default.
  }
}

function readDensity(): Density {
  try {
    return localStorage.getItem(DENSITY_KEY) === "detailed" ? "detailed" : "compact";
  } catch {
    return "compact";
  }
}

/** How tall the rows of the graph are: compact, or detailed (author and date under each line). */
const density = ref<Density>(readDensity());

/** Show the initials of the author in the stations of the graph (on unless switched off). */
const showInitials = ref(readInitials());

export function useGraphSettings() {
  function setDensity(value: Density) {
    density.value = value;
    try {
      localStorage.setItem(DENSITY_KEY, value);
    } catch {
      // Not kept for the next session.
    }
  }
  function setShowInitials(value: boolean) {
    showInitials.value = value;
    try {
      localStorage.setItem(INITIALS_KEY, value ? "on" : "off");
    } catch {
      // Not kept for the next session.
    }
  }
  return { showInitials, setShowInitials, density, setDensity };
}
