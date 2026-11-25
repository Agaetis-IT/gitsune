import {
  type CommitOpResult,
  type DiscardResult,
  type DroppedStash,
  GitCommandError,
  type ResetMode,
  type ReflogEntry,
  type ResetResult,
  type UndoResult,
  type StashEntry,
  type WorkingStatus,
} from "@/lib/api";

interface Hooks {
  status: () => Promise<WorkingStatus>;
  /** Throws the unstaged changes away (the stash took them). */
  clearChanges: () => Promise<void>;
  readOnly: () => boolean;
}

/**
 * Stand-in for the stash, cherry-pick, revert and reset in the plain browser. It keeps a list
 * of stashes in memory and answers the other operations with plausible results, so every
 * dialog, notice and Undo of the interface can be tried.
 */
export function createDemoStash(hooks: Hooks) {
  let stashes: StashEntry[] = [];
  let counter = 0;
  const id = (prefix: string) => `${prefix}${String(++counter).padStart(2, "0")}`.padEnd(40, "0");

  const guard = () => {
    if (hooks.readOnly()) {
      throw new GitCommandError("readOnly", "This repository is in read-only mode.");
    }
  };
  const reindex = () =>
    (stashes = stashes.map((entry, index) => ({
      ...entry,
      index,
      reference: `stash@{${index}}`,
    })));
  const find = (reference: string, stashId: string) => {
    const found = stashes.find((s) => s.reference === reference && s.id === stashId);
    if (!found) {
      throw new GitCommandError(
        "other",
        "The list of stashes changed: reload it and try again.",
      );
    }
    return found;
  };

  return {
    stashes: async () => stashes,
    stashPush: async (_repoId: string, message: string | null) => {
      guard();
      const status = await hooks.status();
      if (status.staged.length + status.unstaged.length === 0) {
        throw new GitCommandError("nothingToCommit", "There is nothing to stash.");
      }
      await hooks.clearChanges();
      stashes = [
        {
          index: 0,
          reference: "stash@{0}",
          id: id("5a"),
          message: message ?? `WIP on ${status.branch ?? "HEAD"}: demo`,
          time: Math.floor(Date.now() / 1000),
        },
        ...stashes,
      ];
      reindex();
      return hooks.status();
    },
    stashApply: async (_repoId: string, reference: string, stashId: string) => {
      guard();
      find(reference, stashId);
      return hooks.status();
    },
    stashPop: async (_repoId: string, reference: string, stashId: string) => {
      guard();
      const found = find(reference, stashId);
      stashes = stashes.filter((s) => s !== found);
      reindex();
      return hooks.status();
    },
    stashDrop: async (_repoId: string, reference: string, stashId: string): Promise<DroppedStash> => {
      guard();
      const found = find(reference, stashId);
      stashes = stashes.filter((s) => s !== found);
      reindex();
      return { id: found.id, message: found.message, status: await hooks.status() };
    },
    stashRestore: async (_repoId: string, stashId: string, message: string) => {
      guard();
      stashes = [
        { index: 0, reference: "stash@{0}", id: stashId, message, time: Math.floor(Date.now() / 1000) },
        ...stashes,
      ];
      reindex();
      return hooks.status();
    },
    cherryPick: async (_repoId: string, commit: string): Promise<CommitOpResult> => {
      guard();
      const status = await hooks.status();
      if (commit.startsWith("c0")) {
        throw new GitCommandError(
          "conflict",
          "The cherry-pick conflicts with the current branch, so it was cancelled and nothing changed.",
        );
      }
      return { status, before: status.head ?? id("be"), after: id("af") };
    },
    revert: async (): Promise<CommitOpResult> => {
      guard();
      const status = await hooks.status();
      return { status, before: status.head ?? id("be"), after: id("af") };
    },
    undoReplay: async () => {
      guard();
      return hooks.status();
    },
    reset: async (_repoId: string, target: string, mode: ResetMode): Promise<ResetResult> => {
      guard();
      const status = await hooks.status();
      const dirty = status.staged.length + status.unstaged.length > 0;
      const snapshot =
        mode === "hard" && dirty
          ? {
              reference: "refs/git-hydra/undo/demo-1",
              id: id("5b"),
              time: Math.floor(Date.now() / 1000),
              description: "reset --hard",
              paths: [...status.staged, ...status.unstaged].map((e) => e.path),
            }
          : null;
      return { status, before: status.head ?? id("be"), after: target, snapshot };
    },
    reflog: async (): Promise<ReflogEntry[]> => {
      const now = Math.floor(Date.now() / 1000);
      return [
        { id: id("c1"), message: "commit: Add the search overlay", time: now - 120, current: true },
        { id: id("c2"), message: "checkout: moving from main to feat/search", time: now - 3600, current: false },
        { id: id("c3"), message: "pull: Fast-forward", time: now - 86400, current: false },
        { id: id("c4"), message: "rebase (finish): returning to refs/heads/main", time: now - 172800, current: false },
      ];
    },
    restoreHead: async (): Promise<WorkingStatus> => {
      guard();
      return hooks.status();
    },
    undoLast: async (): Promise<UndoResult> => {
      guard();
      return { status: await hooks.status(), undone: "the commit" };
    },
    undoReset: async (): Promise<DiscardResult> => {
      guard();
      return { snapshot: null, status: await hooks.status() };
    },
  };
}
