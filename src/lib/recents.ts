export interface RecentRepo {
  path: string;
  name: string;
  /** Milliseconds since the Unix epoch. */
  openedAt: number;
}

type Storage = Pick<globalThis.Storage, "getItem" | "setItem">;

const KEY = "git-hydra:recent-repos";
const MAX_RECENTS = 8;

const isRecent = (value: unknown): value is RecentRepo =>
  typeof value === "object" &&
  value !== null &&
  typeof (value as RecentRepo).path === "string" &&
  typeof (value as RecentRepo).name === "string" &&
  typeof (value as RecentRepo).openedAt === "number";

/** Most-recently-opened repositories, kept in `storage` (best effort: failures are ignored). */
export function createRecentStore(storage: Storage | null) {
  function read(): RecentRepo[] {
    try {
      const parsed: unknown = JSON.parse(storage?.getItem(KEY) ?? "[]");
      return Array.isArray(parsed) ? parsed.filter(isRecent) : [];
    } catch {
      return [];
    }
  }

  function write(items: RecentRepo[]): RecentRepo[] {
    try {
      storage?.setItem(KEY, JSON.stringify(items));
    } catch {
      // Storage unavailable or full: the list just won't persist.
    }
    return items;
  }

  return {
    list: read,
    /** Adds (or moves to the top) a repository, keeping the newest first. */
    add(entry: RecentRepo): RecentRepo[] {
      const others = read().filter((item) => item.path !== entry.path);
      return write([entry, ...others].slice(0, MAX_RECENTS));
    },
    remove(path: string): RecentRepo[] {
      return write(read().filter((item) => item.path !== path));
    },
  };
}

export type RecentStore = ReturnType<typeof createRecentStore>;
