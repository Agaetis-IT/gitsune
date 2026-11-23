import { ref } from "vue";

const INITIALS_KEY = "git-hydra:graph-initials";

function readInitials(): boolean {
  try {
    return localStorage.getItem(INITIALS_KEY) !== "off";
  } catch {
    return true; // Storage unavailable: the default.
  }
}

/** Show the initials of the author in the stations of the graph (on unless switched off). */
const showInitials = ref(readInitials());

export function useGraphSettings() {
  function setShowInitials(value: boolean) {
    showInitials.value = value;
    try {
      localStorage.setItem(INITIALS_KEY, value ? "on" : "off");
    } catch {
      // Not kept for the next session.
    }
  }
  return { showInitials, setShowInitials };
}
