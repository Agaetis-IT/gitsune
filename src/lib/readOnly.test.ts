import { describe, expect, it } from "vitest";
import { createReadOnlyStore } from "./readOnly";

function memoryStorage(initial?: string) {
  const data = new Map<string, string>();
  if (initial !== undefined) data.set("gitsune:read-only-repos", initial);
  return {
    getItem: (key: string) => data.get(key) ?? null,
    setItem: (key: string, value: string) => void data.set(key, value),
  };
}

describe("read-only repositories", () => {
  it("remembers which repositories are read-only", () => {
    const store = createReadOnlyStore(memoryStorage());
    store.set("/a", true);
    store.set("/b", true);
    store.set("/a", false);
    expect(store.has("/a")).toBe(false);
    expect(store.has("/b")).toBe(true);
  });

  it("does not list a repository twice", () => {
    const storage = memoryStorage();
    const store = createReadOnlyStore(storage);
    store.set("/a", true);
    store.set("/a", true);
    expect(JSON.parse(storage.getItem("gitsune:read-only-repos") ?? "[]")).toEqual(["/a"]);
  });

  it("ignores corrupt data and missing storage", () => {
    expect(createReadOnlyStore(memoryStorage("not json")).has("/a")).toBe(false);
    expect(createReadOnlyStore(memoryStorage('{"a":1}')).has("/a")).toBe(false);
    expect(createReadOnlyStore(memoryStorage('[1,"/a",null]')).has("/a")).toBe(true);
    const none = createReadOnlyStore(null);
    expect(() => none.set("/a", true)).not.toThrow();
    expect(none.has("/a")).toBe(false);
  });
});
