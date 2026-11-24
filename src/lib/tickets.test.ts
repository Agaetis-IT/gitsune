import { describe, expect, it } from "vitest";
import type { HostedRepo } from "./remoteUrl";
import { splitTickets, ticketFromBranch, ticketPrefix, ticketUrl } from "./tickets";

const azure: HostedRepo = { kind: "azure", org: "acme", project: "Demo App", repo: "demo" };
const github: HostedRepo = { kind: "github", owner: "acme", repo: "demo" };
const gitlab: HostedRepo = { kind: "gitlab", host: "gitlab.com", path: "group/sub/demo" };

describe("ticketFromBranch", () => {
  it("reads the number of the last segment", () => {
    expect(ticketFromBranch("feat/login-page/5310")).toBe("5310");
    expect(ticketFromBranch("5310-login")).toBe("5310");
    expect(ticketFromBranch("fix/login-5310")).toBe("5310");
    expect(ticketFromBranch("bug/12_title")).toBe("12");
  });

  it("finds nothing in names without a ticket", () => {
    for (const name of ["main", "release/2.1", "feat/v2", "5", "feat/5310/login", ""]) {
      expect(ticketFromBranch(name)).toBeNull();
    }
  });
});

describe("ticketUrl and ticketPrefix", () => {
  it("builds the page of each hosting service", () => {
    expect(ticketUrl(azure, "5310")).toBe(
      "https://dev.azure.com/acme/Demo%20App/_workitems/edit/5310",
    );
    expect(ticketUrl(github, "42")).toBe("https://github.com/acme/demo/issues/42");
    expect(ticketUrl(gitlab, "7")).toBe("https://gitlab.com/group/sub/demo/-/issues/7");
  });

  it("uses AB# on Azure only", () => {
    expect(ticketPrefix(azure)).toBe("AB#");
    expect(ticketPrefix(github)).toBe("#");
    expect(ticketPrefix(gitlab)).toBe("#");
  });
});

describe("splitTickets", () => {
  it("cuts the references out of a message", () => {
    expect(splitTickets("Fix login AB#5310 and #12.")).toEqual([
      { text: "Fix login " },
      { text: "AB#5310", ticket: "5310" },
      { text: " and " },
      { text: "#12", ticket: "12" },
      { text: "." },
    ]);
  });

  it("leaves other text alone", () => {
    expect(splitTickets("Plain &#38; entity, a#b, color #fff")).toEqual([
      { text: "Plain &#38; entity, a#b, color #fff" },
    ]);
    expect(splitTickets("")).toEqual([]);
  });
});
