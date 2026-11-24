import type { BranchUpdate } from "./api";

/** The intervals offered in the settings, in minutes; 0 switches the automatic fetch off. */
export const AUTO_FETCH_CHOICES = [0, 1, 5, 15, 30] as const;

export const isAutoFetchChoice = (value: number): boolean =>
  (AUTO_FETCH_CHOICES as readonly number[]).includes(value);

/** What the scheduler needs to know about an open repository. */
export interface FetchCandidate {
  /** Milliseconds since the Unix epoch of the last automatic fetch (0: never). */
  autoFetchedAt: number;
  autoFetching: boolean;
  /** A fetch, pull or push is running. */
  remoteBusy: boolean;
  /** A write is running. */
  busy: boolean;
}

/**
 * The repositories to fetch now: those whose last automatic fetch is older than the interval,
 * and that are not in the middle of something else. Nothing is due while the setting is off.
 */
export function dueForFetch<T extends FetchCandidate>(
  tabs: readonly T[],
  minutes: number,
  now: number,
): T[] {
  if (minutes <= 0) return [];
  return tabs.filter(
    (tab) =>
      !tab.autoFetching &&
      !tab.remoteBusy &&
      !tab.busy &&
      now - tab.autoFetchedAt >= minutes * 60_000,
  );
}

/** The notification for what a fetch brought: one line for the whole repository. */
export function describeUpdates(
  repoName: string,
  updates: readonly BranchUpdate[],
): { title: string; body: string } | null {
  const total = updates.reduce((sum, update) => sum + update.newCommits, 0);
  if (total === 0) return null;
  const commits = `${total} new commit${total === 1 ? "" : "s"}`;
  const where =
    updates.length === 1 ? `on ${updates[0].upstream}` : `on ${updates.length} branches`;
  return { title: repoName, body: `${commits} ${where}` };
}
