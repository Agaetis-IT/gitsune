import { computed, ref, watch } from "vue";
import { usePlatform } from "./usePlatform";

const EDITOR_KEY = "gitsune:editor";
const TERMINAL_KEY = "gitsune:terminal";

function read(key: string, allowed: ReadonlyArray<{ id: string }>): string {
  try {
    const value = localStorage.getItem(key);
    if (allowed.some((item) => item.id === value)) return value as string;
  } catch {
    // Storage unavailable: the default.
  }
  return allowed[0].id;
}

function write(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Not kept for the next session.
  }
}

const platform = usePlatform().info;
/** The editors and terminals the system offers (they differ between macOS, Linux and Windows). */
const editors = computed(() => platform.value.editors);
const terminals = computed(() => platform.value.terminals);

/** The editor and the terminal the application opens (the first of each list unless told otherwise). */
const editor = ref(read(EDITOR_KEY, editors.value));
const terminal = ref(read(TERMINAL_KEY, terminals.value));

// The system answers after the first read: choose again among what it really offers.
watch(platform, () => {
  editor.value = read(EDITOR_KEY, editors.value);
  terminal.value = read(TERMINAL_KEY, terminals.value);
});

export function useIntegrations() {
  function setEditor(id: string) {
    if (!editors.value.some((item) => item.id === id)) return;
    editor.value = id;
    write(EDITOR_KEY, id);
  }
  function setTerminal(id: string) {
    if (!terminals.value.some((item) => item.id === id)) return;
    terminal.value = id;
    write(TERMINAL_KEY, id);
  }
  const editorLabel = () =>
    editors.value.find((item) => item.id === editor.value)?.label ?? "the editor";
  const terminalLabel = () =>
    terminals.value.find((item) => item.id === terminal.value)?.label ?? "the terminal";
  return {
    editor,
    setEditor,
    terminal,
    setTerminal,
    editors,
    terminals,
    editorLabel,
    terminalLabel,
  };
}
