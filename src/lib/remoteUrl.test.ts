import { describe, expect, it } from "vitest";
import {
  createPullRequestUrl,
  parseRemoteUrl,
  pickHostedRepo,
  pullRequestUrl,
  tokenHost,
} from "./remoteUrl";

const azure = { kind: "azure", org: "org", project: "My Project", repo: "repo" } as const;

describe("parseRemoteUrl", () => {
  it("reads GitHub in its three shapes", () => {
    const expected = { kind: "github", owner: "acme", repo: "tool" };
    expect(parseRemoteUrl("https://github.com/acme/tool.git")).toEqual(expected);
    expect(parseRemoteUrl("git@github.com:acme/tool.git")).toEqual(expected);
    expect(parseRemoteUrl("ssh://git@github.com/acme/tool")).toEqual(expected);
  });

  it("reads Azure DevOps over ssh and https", () => {
    const expected = { kind: "azure", org: "aiutestmgmt", project: "ADS", repo: "pace-frontend" };
    expect(parseRemoteUrl("git@ssh.dev.azure.com:v3/aiutestmgmt/ADS/pace-frontend")).toEqual(
      expected,
    );
    expect(parseRemoteUrl("https://dev.azure.com/aiutestmgmt/ADS/_git/pace-frontend")).toEqual(
      expected,
    );
    expect(
      parseRemoteUrl("https://aiutestmgmt@dev.azure.com/aiutestmgmt/ADS/_git/pace-frontend"),
    ).toEqual(expected);
  });

  it("reads the old visualstudio.com addresses and encoded names", () => {
    expect(parseRemoteUrl("https://org.visualstudio.com/DefaultCollection/P/_git/r")).toEqual({
      kind: "azure",
      org: "org",
      project: "P",
      repo: "r",
    });
    expect(parseRemoteUrl("https://dev.azure.com/org/My%20Project/_git/repo")).toEqual(azure);
  });

  it("gives null for unknown hosts and malformed paths", () => {
    expect(parseRemoteUrl("https://gitlab.com/a/b.git")).toBeNull();
    expect(parseRemoteUrl("git@github.com:onlyone")).toBeNull();
    expect(parseRemoteUrl("https://dev.azure.com/org/project")).toBeNull();
    expect(parseRemoteUrl("/local/path/repo")).toBeNull();
    expect(parseRemoteUrl("")).toBeNull();
  });
});

describe("pull request addresses", () => {
  it("builds the page that starts a pull request", () => {
    expect(createPullRequestUrl({ kind: "github", owner: "a", repo: "b" }, "feature/x y")).toBe(
      "https://github.com/a/b/compare/feature/x%20y?expand=1",
    );
    expect(createPullRequestUrl(azure, "feature/x")).toBe(
      "https://dev.azure.com/org/My%20Project/_git/repo/pullrequestcreate?sourceRef=feature%2Fx",
    );
  });

  it("builds the address of an existing pull request", () => {
    expect(pullRequestUrl({ kind: "github", owner: "a", repo: "b" }, 7)).toBe(
      "https://github.com/a/b/pull/7",
    );
    expect(pullRequestUrl(azure, 12)).toBe(
      "https://dev.azure.com/org/My%20Project/_git/repo/pullrequest/12",
    );
  });

  it("tells the token host", () => {
    expect(tokenHost({ kind: "github", owner: "a", repo: "b" })).toBe("github.com");
    expect(tokenHost(azure)).toBe("dev.azure.com");
  });
});

describe("pickHostedRepo", () => {
  it("prefers origin and skips unknown hosts", () => {
    const picked = pickHostedRepo([
      { name: "backup", url: "git@github.com:a/mirror.git" },
      { name: "other", url: "https://gitlab.com/a/b" },
      { name: "origin", url: "git@ssh.dev.azure.com:v3/o/p/r" },
    ]);
    expect(picked?.remote).toBe("origin");
    expect(picked?.repo.kind).toBe("azure");
    expect(pickHostedRepo([{ name: "x", url: "https://gitlab.com/a/b" }])).toBeNull();
    expect(pickHostedRepo([])).toBeNull();
  });
});
