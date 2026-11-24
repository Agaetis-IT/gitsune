import { describe, expect, it } from "vitest";
import { describeUpdates, dueForFetch, isAutoFetchChoice } from "./autoFetch";

const tab = (overrides: Partial<Parameters<typeof dueForFetch>[0][number]> = {}) => ({
  autoFetchedAt: 0,
  autoFetching: false,
  remoteBusy: false,
  busy: false,
  ...overrides,
});

const MINUTE = 60_000;

describe("dueForFetch", () => {
  it("is empty while the setting is off", () => {
    expect(dueForFetch([tab()], 0, 10 * MINUTE)).toEqual([]);
  });

  it("picks the repositories whose last fetch is older than the interval", () => {
    const old = tab({ autoFetchedAt: 0 });
    const fresh = tab({ autoFetchedAt: 9 * MINUTE });
    expect(dueForFetch([old, fresh], 5, 10 * MINUTE)).toEqual([old]);
    expect(dueForFetch([fresh], 5, 14 * MINUTE)).toEqual([fresh]);
  });

  it("leaves alone a repository that is busy with something else", () => {
    const tabs = [tab({ autoFetching: true }), tab({ remoteBusy: true }), tab({ busy: true })];
    expect(dueForFetch(tabs, 1, 10 * MINUTE)).toEqual([]);
  });
});

describe("describeUpdates", () => {
  it("says nothing when no tracked branch moved", () => {
    expect(describeUpdates("demo", [])).toBeNull();
  });

  it("names the branch when there is one, and counts them otherwise", () => {
    expect(
      describeUpdates("demo", [{ branch: "main", upstream: "origin/main", newCommits: 1 }]),
    ).toEqual({ title: "demo", body: "1 new commit on origin/main" });
    expect(
      describeUpdates("demo", [
        { branch: "main", upstream: "origin/main", newCommits: 2 },
        { branch: "dev", upstream: "origin/dev", newCommits: 3 },
      ]),
    ).toEqual({ title: "demo", body: "5 new commits on 2 branches" });
  });
});

describe("isAutoFetchChoice", () => {
  it("accepts the intervals of the settings only", () => {
    expect(isAutoFetchChoice(5)).toBe(true);
    expect(isAutoFetchChoice(0)).toBe(true);
    expect(isAutoFetchChoice(7)).toBe(false);
  });
});
