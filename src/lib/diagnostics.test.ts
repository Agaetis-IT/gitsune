import { describe, expect, it } from "vitest";
import type { Diagnostics } from "./api";
import { buildReport, findings, type SettingsSnapshot } from "./diagnostics";

const healthy = (): Diagnostics => ({
  appVersion: "0.1.0",
  os: "macos",
  arch: "aarch64",
  pathDirs: ["/opt/homebrew/bin", "~/.cargo/bin", "/usr/bin"],
  tools: [
    { name: "git", found: true, path: "/usr/bin/git", version: "git version 2.50.1" },
    { name: "ssh", found: true, path: "/usr/bin/ssh", version: "OpenSSH_9.9" },
    { name: "az", found: false, path: null, version: null },
  ],
  hosts: [
    { host: "github.com", tool: "GitHub CLI", state: "signedIn" },
    { host: "dev.azure.com", tool: "Azure CLI", state: "missing" },
  ],
  sshAgent: "1Password",
  sshKeys: 2,
  signing: { format: "ssh", enabled: true, hasKey: true },
});

const settings: SettingsSnapshot = {
  restoreTabs: true,
  ticketPrefill: true,
  autoFetchMinutes: 5,
  editor: "zed",
  terminal: "terminal",
};

describe("findings", () => {
  it("finds nothing wrong with a healthy setup", () => {
    expect(findings(healthy())).toEqual([]);
  });

  it("tells about a missing git", () => {
    const d = healthy();
    d.tools[0] = { name: "git", found: false, path: null, version: null };
    expect(findings(d)[0]).toContain("git was not found");
  });

  it("explains an agent without keys, and the Finder case", () => {
    const d = healthy();
    d.sshKeys = 0;
    d.sshAgent = "system (launchd)";
    const [first, second] = findings(d);
    expect(first).toContain("holds no key");
    expect(first).toContain("start it from a terminal");
    expect(first).toContain("system agent");
    expect(second).toContain("signed with an ssh key and the agent holds none");
  });

  it("tells about an unreachable agent, signing without a key and a signed-out tool", () => {
    const d = healthy();
    d.sshKeys = null;
    d.signing = { format: null, enabled: true, hasKey: false };
    d.hosts[0].state = "signedOut";
    const text = findings(d).join("\n");
    expect(text).toContain("cannot be reached");
    expect(text).toContain("no signing key is set");
    expect(text).toContain("signed out of github.com");
  });

  it("does not worry about an optional tool that is not installed", () => {
    expect(findings(healthy()).join(" ")).not.toContain("Azure");
  });
});

describe("buildReport", () => {
  it("writes every section, in Markdown", () => {
    const report = buildReport(healthy(), settings);
    for (const heading of [
      "# git-hydra diagnostics",
      "## To check",
      "## Tools",
      "## Folders searched for tools",
      "## Sign-in",
      "## ssh and signing",
      "## Settings",
    ]) {
      expect(report).toContain(heading);
    }
    expect(report).toContain("Nothing found.");
    expect(report).toContain("| git | yes | git version 2.50.1 | /usr/bin/git |");
    expect(report).toContain("- GitHub CLI (github.com): signed in");
    expect(report).toContain("- Azure CLI (dev.azure.com): not installed");
    expect(report).toContain("- Automatic fetch: every 5 min");
    expect(report.endsWith("\n")).toBe(true);
  });

  it("lists what to check first", () => {
    const d = healthy();
    d.sshKeys = 0;
    expect(buildReport(d, settings)).toMatch(/## To check\n\n- The ssh agent/);
  });

  it("holds only what the diagnostics hold: no account, no address", () => {
    const report = buildReport(healthy(), settings);
    expect(report).not.toMatch(/@|https?:\/\//);
  });
});
