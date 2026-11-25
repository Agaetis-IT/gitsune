import { createApp } from "vue";
import App from "./App.vue";
import { useWorkspace } from "@/composables/useWorkspace";
import { isTauri, tauriBackend } from "@/lib/backend";
import "./assets/main.css";

async function start() {
  // In a plain browser during development there is no Tauri backend: use recorded demo data.
  const demo = import.meta.env.DEV && !isTauri();
  const backend = demo
    ? await import("@/dev/fixtureBackend").then((m) => m.createFixtureBackend())
    : tauriBackend;
  const workspace = useWorkspace(backend);
  createApp(App).mount("#app");
  if (demo) await workspace.openPath("demo-repo");
}

void start();
