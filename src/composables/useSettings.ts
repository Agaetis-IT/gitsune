import { ref } from "vue";

/** Whether the settings window is open (one window for the whole app). */
const open = ref(false);

export function useSettings() {
  return {
    open,
    openSettings: () => (open.value = true),
    closeSettings: () => (open.value = false),
  };
}
