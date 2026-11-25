import type { Diagnostics } from "./api";

/** The settings of the application that are worth putting in the report. */
export interface SettingsSnapshot {
  restoreTabs: boolean;
  ticketPrefill: boolean;
  autoFetchMinutes: number;
  editor: string;
  terminal: string;
}

const tool = (d: Diagnostics, name: string) => d.tools.find((t) => t.name === name);

/**
 * What looks wrong, in plain words and in the order it matters. Empty when nothing does. Every
 * line says what may fail and what to do about it.
 */
export function findings(d: Diagnostics): string[] {
  const found: string[] = [];
  if (!tool(d, "git")?.found) {
    found.push(
      "git was not found in the folders the app looks in: install it, or add its folder to your PATH (an app started from the Finder does not read your shell configuration).",
    );
  }
  if (!tool(d, "ssh")?.found) {
    found.push("ssh was not found: remotes that use ssh cannot be reached from the app.");
  }
  if (d.sshKeys === null) {
    found.push(
      `The ssh agent (${d.sshAgent}) cannot be reached: ssh remotes and commit signing with an ssh key may fail.`,
    );
  } else if (d.sshKeys === 0) {
    const finder =
      d.sshAgent === "system (launchd)"
        ? " The app got the system agent, which has none of your keys: start it from a terminal, or name your agent with IdentityAgent in ~/.ssh/config."
        : "";
    found.push(
      `The ssh agent (${d.sshAgent}) holds no key: ssh remotes and commit signing with an ssh key may fail. Add a key with ssh-add.${finder}`,
    );
  }
  if (d.signing.enabled && !d.signing.hasKey) {
    found.push(
      "Commit signing is on (commit.gpgsign) but no signing key is set (user.signingkey): every commit will fail.",
    );
  }
  if (d.signing.enabled && d.signing.format === "ssh" && d.sshKeys === 0) {
    found.push(
      "Commits are signed with an ssh key and the agent holds none: commits, and each commit a rebase replays, will fail.",
    );
  }
  for (const host of d.hosts) {
    if (host.state === "signedOut") {
      found.push(
        `${host.tool} is installed but you are signed out of ${host.host}: pull request badges and creation need it.`,
      );
    }
  }
  return found;
}

const STATE = { signedIn: "signed in", signedOut: "signed out", missing: "not installed" } as const;

const cell = (text: string): string => text.replace(/\|/g, "\\|");

/** The report as Markdown, ready to be pasted in a ticket. */
export function buildReport(d: Diagnostics, settings: SettingsSnapshot): string {
  const lines: string[] = [];
  lines.push("# Gitsune diagnostics", "");
  lines.push(`- Application: ${d.appVersion}`);
  lines.push(`- System: ${d.os} ${d.arch}`, "");

  lines.push("## To check", "");
  const problems = findings(d);
  if (problems.length === 0) lines.push("Nothing found.");
  else lines.push(...problems.map((p) => `- ${p}`));

  lines.push("", "## Tools", "", "| Tool | Found | Version | Path |", "| --- | --- | --- | --- |");
  for (const t of d.tools) {
    lines.push(
      `| ${t.name} | ${t.found ? "yes" : "no"} | ${cell(t.version ?? "")} | ${cell(t.path ?? "")} |`,
    );
  }

  lines.push("", "## Folders searched for tools", "");
  lines.push(...d.pathDirs.map((dir, i) => `${i + 1}. ${dir}`));

  lines.push("", "## Sign-in", "");
  lines.push(...d.hosts.map((h) => `- ${h.tool} (${h.host}): ${STATE[h.state]}`));

  lines.push("", "## ssh and signing", "");
  lines.push(`- ssh agent: ${d.sshAgent}`);
  lines.push(`- Keys in the agent: ${d.sshKeys === null ? "unknown (not reachable)" : d.sshKeys}`);
  lines.push(
    `- Commit signing: ${d.signing.enabled ? "on" : "off"}, format ${d.signing.format ?? "default"}, signing key ${d.signing.hasKey ? "set" : "not set"}`,
  );

  lines.push("", "## Settings", "");
  lines.push(`- Restore the tabs at start: ${settings.restoreTabs ? "on" : "off"}`);
  lines.push(`- Ticket in the commit message: ${settings.ticketPrefill ? "on" : "off"}`);
  lines.push(
    `- Automatic fetch: ${settings.autoFetchMinutes === 0 ? "never" : `every ${settings.autoFetchMinutes} min`}`,
  );
  lines.push(`- Editor: ${settings.editor}`, `- Terminal: ${settings.terminal}`);
  return `${lines.join("\n")}\n`;
}
