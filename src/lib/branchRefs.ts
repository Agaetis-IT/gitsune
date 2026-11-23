import type { RefInfo } from "@/lib/api";

/** `origin/feature/x` -> `feature/x` (the remote name is the first path segment). */
export const stripRemote = (name: string): string => {
  const slash = name.indexOf("/");
  return slash === -1 ? name : name.slice(slash + 1);
};

const branchKey = (ref: RefInfo): string =>
  ref.kind === "remote" ? stripRemote(ref.name) : ref.name;

/**
 * Every ref that stands for the branch of a lane: the local branch and its remote-tracking
 * branches (`main`, `origin/main`, ...). Tags are never grouped. Empty when no ref has that name.
 */
export function refsOfBranch(refs: readonly RefInfo[], laneName: string): RefInfo[] {
  const named = refs.find((ref) => ref.name === laneName);
  if (!named) return [];
  if (named.kind === "tag") return [named];
  const key = branchKey(named);
  return refs.filter((ref) => ref.kind !== "tag" && branchKey(ref) === key);
}
