import { describe, expect, it } from "vitest";
import { repoNameFromUrl } from "./cloneUrl";

describe("repoNameFromUrl", () => {
  it("reads the name of every shape of address", () => {
    expect(repoNameFromUrl("https://github.com/acme/tool.git")).toBe("tool");
    expect(repoNameFromUrl("git@github.com:acme/tool.git")).toBe("tool");
    expect(repoNameFromUrl("ssh://git@host:2222/team/app")).toBe("app");
    expect(repoNameFromUrl("https://dev.azure.com/org/Proj/_git/repo")).toBe("repo");
    expect(repoNameFromUrl("/srv/git/local.git/")).toBe("local");
  });

  it("gives an empty name when there is nothing to read", () => {
    expect(repoNameFromUrl("")).toBe("");
    expect(repoNameFromUrl("   ")).toBe("");
  });
});
