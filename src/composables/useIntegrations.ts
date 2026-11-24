import { ref } from "vue";

export const EDITORS = [
  { id: "zed", label: "Zed" },
  { id: "vscode", label: "Visual Studio Code" },
  { id: "cursor", label: "Cursor" },
] as const;

export const TERMINALS = [
  { id: "terminal", label: "Terminal" },
  { id: "iterm", label: "iTerm" },
  { id: "ghostty", label: "Ghostty" },
] as const;

const EDITOR_KEY = "git-hydra:editor";
const TERMINAL_KEY = "git-hydra:terminal";

function read(key: string, allowed: ReadonlyArray<{ id: string }>, fallback: string): string {
  try {
    const value = localStorage.getItem(key);
    return allowed.some((item) => item.id === value) ? (value as string) : fallback;
  } catch {
    return fallback;
  }
}

function write(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Not kept for the next session.
  }
}

/** The editor and the terminal the application opens (Zed and Terminal unless told otherwise). */
const editor = ref(read(EDITOR_KEY, EDITORS, "zed"));
const terminal = ref(read(TERMINAL_KEY, TERMINALS, "terminal"));

export function useIntegrations() {
  function setEditor(id: string) {
    if (!EDITORS.some((item) => item.id === id)) return;
    editor.value = id;
    write(EDITOR_KEY, id);
  }
  function setTerminal(id: string) {
    if (!TERMINALS.some((item) => item.id === id)) return;
    terminal.value = id;
    write(TERMINAL_KEY, id);
  }
  const editorLabel = () => EDITORS.find((item) => item.id === editor.value)?.label ?? "the editor";
  const terminalLabel = () =>
    TERMINALS.find((item) => item.id === terminal.value)?.label ?? "the terminal";
  return { editor, setEditor, terminal, setTerminal, editorLabel, terminalLabel };
}
