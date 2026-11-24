import type { RefInfo, Worktree } from "./api";

const trim = (path: string): string => path.replace(/\/+$/, "");

export const samePath = (a: string, b: string): boolean => trim(a) === trim(b);

/** The last part of a folder path. */
export const folderName = (path: string): string => trim(path).split("/").pop() ?? path;

/** The folder above `path`. */
export const parentFolder = (path: string): string => {
  const parts = trim(path).split("/");
  parts.pop();
  return parts.join("/") || "/";
};

/** A branch name as a folder name: `feat/login-page` gives `feat-login-page`. */
export const flatten = (branch: string): string =>
  branch
    .replace(/[^A-Za-z0-9._-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .replace(/-{2,}/g, "-");

/** Where a new working tree goes by default: next to the repository, `<repo>-<branch>`. */
export function defaultWorktreePath(repoPath: string, branch: string): string {
  const flat = flatten(branch);
  return `${parentFolder(repoPath)}/${folderName(repoPath)}${flat ? `-${flat}` : ""}`;
}

/** The other working tree that has `branch` checked out, if there is one. */
export function checkedOutElsewhere(
  worktrees: readonly Worktree[],
  currentPath: string,
  branch: string,
): Worktree | undefined {
  return worktrees.find((tree) => tree.branch === branch && !samePath(tree.path, currentPath));
}

/** The local branches no working tree has checked out: the ones a new working tree can take. */
export function freeBranches(refs: readonly RefInfo[], worktrees: readonly Worktree[]): string[] {
  const used = new Set(worktrees.map((tree) => tree.branch).filter((b): b is string => !!b));
  return refs.filter((ref) => ref.kind === "local" && !used.has(ref.name)).map((ref) => ref.name);
}

/** What git says when a branch is checked out in another working tree: the folder, or `null`. */
export function worktreeOfRefusal(message: string): string | null {
  const match = /already (?:checked out|used by worktree) at '([^']+)'/.exec(message);
  return match ? match[1] : null;
}
