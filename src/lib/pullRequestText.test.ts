import { describe, expect, it } from "vitest";
import type { RangeCommit } from "./api";
import { humanizeBranch, pullRequestDescription, pullRequestTitle } from "./pullRequestText";
import type { HostedRepo } from "./remoteUrl";

const azure: HostedRepo = { kind: "azure", org: "acme", project: "Demo", repo: "demo" };
const github: HostedRepo = { kind: "github", owner: "acme", repo: "demo" };
const commit = (summary: string): RangeCommit => ({ id: summary, summary });

describe("humanizeBranch", () => {
  it("drops the type and the ticket, and spaces the words", () => {
    expect(humanizeBranch("feat/login-page/5310")).toBe("Login page");
    expect(humanizeBranch("fix/typo_in_header")).toBe("Typo in header");
    expect(humanizeBranch("5310-login")).toBe("Login");
  });

  it("falls back to the whole name", () => {
    expect(humanizeBranch("main")).toBe("Main");
    expect(humanizeBranch("feat/5310")).toBe("Feat/5310");
  });
});

describe("pullRequestTitle", () => {
  it("uses the only commit, with the ticket in front", () => {
    expect(pullRequestTitle("feat/login/5310", [commit("Add the login form")], azure)).toBe(
      "AB#5310 Add the login form",
    );
    expect(pullRequestTitle("feat/login/5310", [commit("Add the login form")], github)).toBe(
      "#5310 Add the login form",
    );
  });

  it("uses the branch name when there are several commits", () => {
    expect(pullRequestTitle("feat/login-page/5310", [commit("a"), commit("b")], azure)).toBe(
      "AB#5310 Login page",
    );
  });

  it("does not repeat a ticket the message already names", () => {
    expect(pullRequestTitle("feat/login/5310", [commit("AB#5310 Add the form")], azure)).toBe(
      "AB#5310 Add the form",
    );
  });

  it("has no ticket to add on a branch without one", () => {
    expect(pullRequestTitle("feat/login", [commit("Add it")], azure)).toBe("Add it");
  });
});

describe("pullRequestDescription", () => {
  it("never lists the commits", () => {
    expect(pullRequestDescription("feat/x", github)).toBe("");
  });

  it("is the work item reference on Azure only", () => {
    expect(pullRequestDescription("feat/x/5310", azure)).toBe("AB#5310");
    expect(pullRequestDescription("feat/x/5310", github)).toBe("");
    expect(pullRequestDescription("feat/x", azure)).toBe("");
  });
});
