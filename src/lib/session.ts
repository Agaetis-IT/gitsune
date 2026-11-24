/** The tabs open when the application was closed. */
export interface Session {
  /** Paths of the open repositories, in tab order. */
  paths: string[];
  /** Path of the tab that was shown, `null` for none. */
  active: string | null;
  /** Full names of the refs hidden from the graph, by repository path. */
  hidden: Record<string, string[]>;
}

type Storage = Pick<globalThis.Storage, "getItem" | "setItem">;

const KEY = "git-hydra:session";

const isStrings = (value: unknown): value is string[] =>
  Array.isArray(value) && value.every((item) => typeof item === "string");

export const emptySession = (): Session => ({ paths: [], active: null, hidden: {} });

/** The session kept in `storage` (best effort: failures are ignored, a corrupt value is empty). */
export function createSessionStore(storage: Storage | null) {
  return {
    read(): Session {
      try {
        const parsed = JSON.parse(storage?.getItem(KEY) ?? "null") as Partial<Session> | null;
        if (!parsed || !isStrings(parsed.paths)) return emptySession();
        const hidden: Record<string, string[]> = {};
        for (const [path, names] of Object.entries(parsed.hidden ?? {})) {
          if (isStrings(names)) hidden[path] = names;
        }
        return {
          paths: [...new Set(parsed.paths)],
          active: typeof parsed.active === "string" ? parsed.active : null,
          hidden,
        };
      } catch {
        return emptySession();
      }
    },
    write(session: Session): void {
      try {
        storage?.setItem(KEY, JSON.stringify(session));
      } catch {
        // Storage unavailable or full: the tabs just won't come back.
      }
    },
  };
}

export type SessionStore = ReturnType<typeof createSessionStore>;
