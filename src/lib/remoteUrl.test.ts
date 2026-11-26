import { describe, expect, it } from "vitest";
import {
  createPullRequestUrl,
  parseRemoteUrl,
  pickHostedRepo,
  hostOf,
  pullRequestUrl,
  repoLabel,
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

  it("reads GitLab, with subgroups and on a self-hosted instance", () => {
    expect(parseRemoteUrl("https://gitlab.com/group/sub/project.git")).toEqual({
      kind: "gitlab",
      host: "gitlab.com",
      path: "group/sub/project",
    });
    expect(parseRemoteUrl("git@gitlab.example.fr:team/app.git")).toEqual({
      kind: "gitlab",
      host: "gitlab.example.fr",
      path: "team/app",
    });
    expect(parseRemoteUrl("ssh://git@gitlab.example.fr:2222/team/app.git")).toEqual({
      kind: "gitlab",
      host: "gitlab.example.fr",
      path: "team/app",
    });
    expect(parseRemoteUrl("https://gitlab.com/onlyone")).toBeNull();
  });

  it("reads the host an SSH alias resolves to", () => {
    const expected = { kind: "github", owner: "acme", repo: "tool" };
    expect(parseRemoteUrl("git@work-github:acme/tool.git", "github.com")).toEqual(expected);
    expect(parseRemoteUrl("git@work-github:acme/tool.git")).toBeNull();
  });

  it("gives null for unknown hosts and malformed paths", () => {
    expect(parseRemoteUrl("https://bitbucket.org/a/b.git")).toBeNull();
    expect(parseRemoteUrl("https://code.example.com/a/b.git")).toBeNull();
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

  it("builds the GitLab merge request pages", () => {
    const gitlab = { kind: "gitlab", host: "gitlab.com", path: "group/sub/project" } as const;
    expect(createPullRequestUrl(gitlab, "feature/x y")).toBe(
      "https://gitlab.com/group/sub/project/-/merge_requests/new?merge_request%5Bsource_branch%5D=feature%2Fx%20y",
    );
    expect(pullRequestUrl(gitlab, 5)).toBe(
      "https://gitlab.com/group/sub/project/-/merge_requests/5",
    );
  });

  it("tells the host of a repository", () => {
    expect(hostOf({ kind: "github", owner: "a", repo: "b" })).toBe("github.com");
    expect(hostOf(azure)).toBe("dev.azure.com");
    expect(hostOf({ kind: "gitlab", host: "gitlab.example.fr", path: "a/b" })).toBe(
      "gitlab.example.fr",
    );
  });
});

describe("pickHostedRepo", () => {
  it("prefers origin and skips unknown hosts", () => {
    const picked = pickHostedRepo([
      { name: "backup", url: "git@github.com:a/mirror.git" },
      { name: "other", url: "https://bitbucket.org/a/b" },
      { name: "origin", url: "git@ssh.dev.azure.com:v3/o/p/r" },
    ]);
    expect(picked?.remote).toBe("origin");
    expect(picked?.repo.kind).toBe("azure");
    expect(pickHostedRepo([{ name: "x", url: "https://bitbucket.org/a/b" }])).toBeNull();
    expect(pickHostedRepo([])).toBeNull();
  });

  it("uses the resolved host of an SSH alias", () => {
    const picked = pickHostedRepo([
      { name: "origin", url: "git@work-github:acme/tool.git", host: "github.com" },
    ]);
    expect(picked?.repo).toEqual({ kind: "github", owner: "acme", repo: "tool" });
  });
});

describe("repoLabel", () => {
  it("names a repository of each host", () => {
    expect(repoLabel(azure)).toBe("org/My Project/repo");
    expect(repoLabel({ kind: "github", owner: "acme", repo: "tool" })).toBe("acme/tool");
    expect(repoLabel({ kind: "gitlab", host: "gitlab.com", path: "group/sub/app" })).toBe(
      "gitlab.com/group/sub/app",
    );
  });
});
