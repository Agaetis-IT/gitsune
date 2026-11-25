import { ref } from "vue";

/** The file whose context menu is open (the panels that list files set it, the app shows it). */
export const fileMenu = ref<{ path: string; x: number; y: number } | null>(null);

export function useFileMenu() {
  return {
    fileMenu,
    openFileMenu: (path: string, event: MouseEvent) => {
      event.preventDefault();
      fileMenu.value = { path, x: event.clientX, y: event.clientY };
    },
    closeFileMenu: () => (fileMenu.value = null),
  };
}
