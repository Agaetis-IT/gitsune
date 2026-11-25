type Storage = Pick<globalThis.Storage, "getItem" | "setItem">;

const KEY = "gitsune:read-only-repos";

/** Which repositories are in read-only mode, kept in `storage` (best effort). */
export function createReadOnlyStore(storage: Storage | null) {
  function read(): string[] {
    try {
      const parsed: unknown = JSON.parse(storage?.getItem(KEY) ?? "[]");
      return Array.isArray(parsed)
        ? parsed.filter((id): id is string => typeof id === "string")
        : [];
    } catch {
      return [];
    }
  }

  return {
    has: (repoId: string): boolean => read().includes(repoId),
    set(repoId: string, readOnly: boolean): void {
      const others = read().filter((id) => id !== repoId);
      try {
        storage?.setItem(KEY, JSON.stringify(readOnly ? [...others, repoId] : others));
      } catch {
        // Storage unavailable: the mode just won't be remembered.
      }
    },
  };
}

export type ReadOnlyStore = ReturnType<typeof createReadOnlyStore>;
