import {
  type ConflictFile,
  GitCommandError,
  type MergeInfo,
  type MergeMode,
  type MergeResult,
  type WorkingStatus,
} from "@/lib/api";

interface Hooks {
  /** The status without the merge (what the demo working tree holds). */
  plain: () => Promise<WorkingStatus>;
  headId: () => string;
  readOnly: () => boolean;
}

interface DemoConflict {
  path: string;
  code: string;
  file: Omit<ConflictFile, "path" | "code">;
}

const SOURCE = [
  "export function login(user: string) {",
  "<<<<<<< HEAD",
  "  return signIn(user, { remember: true });",
  "=======",
  "  return signIn(user, { remember: false, audit: true });",
  ">>>>>>> feature/auth-18",
  "}",
  "",
  "export function logout() {",
  "<<<<<<< HEAD",
  "  clearSession();",
  "=======",
  "  clearSession();",
  "  notifyServer();",
  ">>>>>>> feature/auth-18",
  "}",
  "",
].join("\n");

const initialConflicts = (): DemoConflict[] => [
  {
    path: "src/auth.ts",
    code: "UU",
    file: {
      ours: null,
      theirs: null,
      base: null,
      worktree: SOURCE,
      chooseSideOnly: false,
    },
  },
  {
    path: "assets/logo.png",
    code: "UU",
    file: { ours: null, theirs: null, base: null, worktree: null, chooseSideOnly: true },
  },
];

/**
 * Stand-in for merging in the plain browser. Merging a `feature/auth-*` branch stops on two
 * conflicts (one text file, one binary) so the banner and the resolver can be tried; any other
 * branch merges cleanly.
 */
export function createDemoMerge(hooks: Hooks) {
  let conflicts: DemoConflict[] | null = null;
  let message = "";
  let counter = 0;
  const id = (prefix: string) => `${prefix}${String(++counter).padStart(2, "0")}`.padEnd(40, "0");

  const guard = () => {
    if (hooks.readOnly()) {
      throw new GitCommandError("readOnly", "This repository is in read-only mode.");
    }
  };

  /** Adds the merge in progress and its conflicts to a status. */
  const decorate = (status: WorkingStatus): WorkingStatus =>
    conflicts === null
      ? status
      : {
          ...status,
          operation: { kind: "merge", message },
          conflicted: conflicts.map(({ path, code }) => ({ path, code })),
        };
  const status = async () => decorate(await hooks.plain());

  const find = (path: string) => {
    const found = conflicts?.find((c) => c.path === path);
    if (!found) throw new GitCommandError("other", `${path} has no conflict (any more).`);
    return found;
  };

  return {
    decorate,
    mergeInfo: async (_repoId: string, target: string): Promise<MergeInfo> => ({
      upToDate: false,
      canFastForward: !target.includes("auth"),
      commits: 3,
    }),
    merge: async (_repoId: string, target: string, mode: MergeMode): Promise<MergeResult> => {
      guard();
      if (conflicts) throw new GitCommandError("other", "Another operation is waiting to be finished.");
      const before = hooks.headId();
      if (target.includes("auth")) {
        conflicts = initialConflicts();
        message = `Merge branch '${target}'`;
        return { status: await status(), outcome: "conflicts", before, after: before };
      }
      const outcome =
        mode === "squash" ? "squashed" : mode === "noFastForward" ? "merged" : "fastForward";
      return { status: await status(), outcome, before, after: id("ab") };
    },
    conflictFile: async (_repoId: string, path: string): Promise<ConflictFile> => {
      const found = find(path);
      return { path, code: found.code, ...found.file };
    },
    resolveFile: async (_repoId: string, path: string, content: string) => {
      guard();
      find(path);
      if (/^<{7}( |$)/m.test(content)) {
        throw new GitCommandError("other", "The file still has conflict markers.");
      }
      conflicts = (conflicts ?? []).filter((c) => c.path !== path);
      return status();
    },
    resolveSide: async (_repoId: string, path: string) => {
      guard();
      find(path);
      conflicts = (conflicts ?? []).filter((c) => c.path !== path);
      return status();
    },
    continueOperation: async () => {
      guard();
      if (conflicts && conflicts.length > 0) {
        throw new GitCommandError(
          "conflict",
          `${conflicts.length} file(s) still have conflicts.`,
        );
      }
      conflicts = null;
      return status();
    },
    abortOperation: async () => {
      guard();
      conflicts = null;
      return status();
    },
    status,
  };
}
