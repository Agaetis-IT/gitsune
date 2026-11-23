import { describe, expect, it } from "vitest";
import { createRecentStore, type RecentRepo } from "./recents";

function memoryStorage(initial?: string) {
  const data = new Map<string, string>();
  if (initial !== undefined) data.set("git-hydra:recent-repos", initial);
  return {
    getItem: (key: string) => data.get(key) ?? null,
    setItem: (key: string, value: string) => void data.set(key, value),
  };
}

const repo = (name: string, openedAt = 1): RecentRepo => ({
  path: `/fake/${name}`,
  name,
  openedAt,
});

describe("recent repositories", () => {
  it("keeps the newest first and moves a reopened repository to the top", () => {
    const store = createRecentStore(memoryStorage());
    store.add(repo("a"));
    store.add(repo("b"));
    store.add(repo("a", 5));
    expect(store.list().map((r) => r.name)).toEqual(["a", "b"]);
    expect(store.list()[0].openedAt).toBe(5);
  });

  it("keeps at most 8 entries", () => {
    const store = createRecentStore(memoryStorage());
    for (let i = 0; i < 12; i++) store.add(repo(`r${i}`));
    const names = store.list().map((r) => r.name);
    expect(names).toHaveLength(8);
    expect(names[0]).toBe("r11");
    expect(names[7]).toBe("r4");
  });

  it("removes an entry", () => {
    const store = createRecentStore(memoryStorage());
    store.add(repo("a"));
    store.add(repo("b"));
    expect(store.remove("/fake/a").map((r) => r.name)).toEqual(["b"]);
  });

  it("ignores corrupt or malformed stored data", () => {
    expect(createRecentStore(memoryStorage("not json")).list()).toEqual([]);
    expect(createRecentStore(memoryStorage('{"a":1}')).list()).toEqual([]);
    const mixed = JSON.stringify([repo("ok"), { path: 1 }, null, "x"]);
    expect(
      createRecentStore(memoryStorage(mixed))
        .list()
        .map((r) => r.name),
    ).toEqual(["ok"]);
  });

  it("works without storage and survives a failing one", () => {
    expect(createRecentStore(null).add(repo("a"))).toEqual([repo("a")]);
    const broken = {
      getItem: () => {
        throw new Error("denied");
      },
      setItem: () => {
        throw new Error("denied");
      },
    };
    const store = createRecentStore(broken);
    expect(store.list()).toEqual([]);
    expect(() => store.add(repo("a"))).not.toThrow();
  });
});
