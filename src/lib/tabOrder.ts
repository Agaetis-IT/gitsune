/**
 * The ids with the one at `from` moved so that it ends up at index `to` (an index of the result,
 * so `to` is where the tab lands whichever side it came from). Out-of-range moves change nothing.
 */
export function moveId(ids: readonly string[], from: number, to: number): string[] {
  if (from < 0 || from >= ids.length || to < 0 || to >= ids.length || from === to) {
    return [...ids];
  }
  const next = [...ids];
  const [id] = next.splice(from, 1);
  next.splice(to, 0, id);
  return next;
}

/**
 * Where a tab dragged from `from` lands when it is dropped in the gap `gap` (0 is before the first
 * tab, `count` after the last), as an index of the result.
 */
export function landingIndex(from: number, gap: number): number {
  return gap > from ? gap - 1 : gap;
}
