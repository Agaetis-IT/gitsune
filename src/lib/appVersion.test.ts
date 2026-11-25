import { describe, expect, it } from "vitest";
import cargoToml from "../../src-tauri/Cargo.toml?raw";
import tauriConf from "../../src-tauri/tauri.conf.json?raw";
import packageJson from "../../package.json?raw";

describe("the version of the application", () => {
  it("is the same in package.json, tauri.conf.json and Cargo.toml", () => {
    const fromPackage = (JSON.parse(packageJson) as { version: string }).version;
    const fromTauri = (JSON.parse(tauriConf) as { version: string }).version;
    const fromCargo = /^version\s*=\s*"([^"]+)"/m.exec(cargoToml)?.[1];
    expect(fromTauri).toBe(fromPackage);
    expect(fromCargo).toBe(fromPackage);
  });
});

// The release workflow stops when the notes of the version are missing: this fails earlier, in the
// pull request that changes the version.
const releaseNotes = import.meta.glob("../../docs/releases/*.md", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

describe("the release notes", () => {
  it("exist for the current version", () => {
    const version = (JSON.parse(packageJson) as { version: string }).version;
    const found = Object.entries(releaseNotes).find(([path]) => path.endsWith(`/${version}.md`));
    expect(found, `docs/releases/${version}.md is missing`).toBeDefined();
    expect(found?.[1]).toContain(`# Gitsune ${version}`);
  });
});
