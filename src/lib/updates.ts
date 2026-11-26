/** The automatic check runs at most once in this time (a day), in milliseconds. */
export const UPDATE_CHECK_INTERVAL = 24 * 60 * 60 * 1000;

/**
 * Whether the automatic check is due: the last successful one is more than a day old, or
 * there is none. A clock set back (a last check in the future) counts as due.
 */
export function updateCheckDue(lastChecked: number, now: number): boolean {
  return lastChecked <= 0 || lastChecked > now || now - lastChecked >= UPDATE_CHECK_INTERVAL;
}
