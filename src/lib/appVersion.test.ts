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
