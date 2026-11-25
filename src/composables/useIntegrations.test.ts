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

  it("offers the editors and terminals of the system, and chooses again when it answers", async () => {
    localStorage.setItem("git-hydra:terminal", "kitty");
    const platform = (await import("./usePlatform")).usePlatform();
    const integrations = (await import("./useIntegrations")).useIntegrations();
    // Until the system answers, the Mac list is assumed and "kitty" is not in it.
    expect(integrations.terminal.value).toBe("terminal");
    platform.info.value = {
      os: "linux",
      editors: [{ id: "zed", label: "Zed" }],
      terminals: [
        { id: "gnome-terminal", label: "GNOME Terminal" },
        { id: "kitty", label: "kitty" },
      ],
      fileManager: "the file manager",
      nativeMenu: false,
    };
    await Promise.resolve();
    expect(integrations.terminals.value.map((t) => t.id)).toEqual(["gnome-terminal", "kitty"]);
    expect(integrations.terminal.value).toBe("kitty");
    expect(integrations.terminalLabel()).toBe("kitty");
    integrations.setTerminal("terminal");
    expect(integrations.terminal.value).toBe("kitty");
  });
});
