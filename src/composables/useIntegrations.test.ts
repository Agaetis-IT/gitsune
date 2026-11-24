import { beforeEach, describe, expect, it, vi } from "vitest";

describe("integrations settings", () => {
  beforeEach(() => {
    vi.resetModules();
    localStorage.clear();
  });

  it("starts with Zed and Terminal and remembers a choice", async () => {
    let s = (await import("./useIntegrations")).useIntegrations();
    expect(s.editor.value).toBe("zed");
    expect(s.terminal.value).toBe("terminal");
    s.setEditor("vscode");
    s.setTerminal("ghostty");
    vi.resetModules();
    s = (await import("./useIntegrations")).useIntegrations();
    expect(s.editor.value).toBe("vscode");
    expect(s.terminal.value).toBe("ghostty");
    expect(s.editorLabel()).toBe("Visual Studio Code");
  });

  it("ignores an application it does not know", async () => {
    localStorage.setItem("git-hydra:editor", "emacs");
    const s = (await import("./useIntegrations")).useIntegrations();
    expect(s.editor.value).toBe("zed");
    s.setEditor("vim");
    expect(s.editor.value).toBe("zed");
  });
});
