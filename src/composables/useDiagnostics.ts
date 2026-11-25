import { ref } from "vue";

/** Whether the diagnostics window is open (one window for the whole app). */
const open = ref(false);

export function useDiagnostics() {
  return {
    open,
    openDiagnostics: () => (open.value = true),
    closeDiagnostics: () => (open.value = false),
  };
}
