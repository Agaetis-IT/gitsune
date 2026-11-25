import { ref } from "vue";
import { getPlatformInfo, type PlatformInfo } from "@/lib/api";

/** What is assumed until the system has answered (and in the plain browser used for development). */
export const DEFAULT_PLATFORM: PlatformInfo = {
  os: "macos",
  editors: [
    { id: "zed", label: "Zed" },
    { id: "vscode", label: "Visual Studio Code" },
    { id: "cursor", label: "Cursor" },
  ],
  terminals: [
    { id: "terminal", label: "Terminal" },
    { id: "iterm", label: "iTerm" },
    { id: "ghostty", label: "Ghostty" },
  ],
  fileManager: "Finder",
  nativeMenu: true,
};

const info = ref<PlatformInfo>(DEFAULT_PLATFORM);

/** The system the application runs on: its editors and terminals, its file manager. */
export function usePlatform() {
  /** Asks the backend once at start; a failure keeps the defaults. */
  async function load(): Promise<void> {
    try {
      info.value = await getPlatformInfo();
    } catch {
      // Outside the desktop application: keep the defaults.
    }
  }
  return { info, load };
}
