import { describe, expect, it } from "vitest";
import { createSessionStore } from "./session";

function memoryStorage(initial?: string) {
  const data = new Map<string, string>();
  if (initial !== undefined) data.set("git-hydra:session", initial);
  return {
    getItem: (key: string) => data.get(key) ?? null,
    setItem: (key: string, value: string) => void data.set(key, value),
  };
}

describe("session store", () => {
  it("gives back what was written", () => {
    const store = createSessionStore(memoryStorage());
    const session = { paths: ["/a", "/b"], active: "/b", hidden: { "/a": ["refs/heads/x"] } };
    store.write(session);
    expect(store.read()).toEqual(session);
  });

  it("starts empty with no storage, a corrupt value or a wrong shape", () => {
    const empty = { paths: [], active: null, hidden: {} };
    expect(createSessionStore(null).read()).toEqual(empty);
    expect(createSessionStore(memoryStorage("nope")).read()).toEqual(empty);
    expect(createSessionStore(memoryStorage('{"paths":[1]}')).read()).toEqual(empty);
  });

  it("drops duplicates and malformed hidden lists", () => {
    const stored = JSON.stringify({
      paths: ["/a", "/a"],
      active: 3,
      hidden: { "/a": ["r"], "/b": [1] },
    });
    expect(createSessionStore(memoryStorage(stored)).read()).toEqual({
      paths: ["/a"],
      active: null,
      hidden: { "/a": ["r"] },
    });
  });

  it("survives a failing storage", () => {
    const broken = {
      getItem: () => {
        throw new Error("denied");
      },
      setItem: () => {
        throw new Error("full");
      },
    };
    const store = createSessionStore(broken);
    expect(() => store.write({ paths: ["/a"], active: null, hidden: {} })).not.toThrow();
    expect(store.read().paths).toEqual([]);
  });
});
