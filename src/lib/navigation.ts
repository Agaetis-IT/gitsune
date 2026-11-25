import type { RefInfo } from "@/lib/api";

/**
 * What a double click on a commit (or on one of its labels) switches to: a local branch that is
 * not checked out yet, else the checked-out one (so the app can say so), else a remote branch
 * (the local branch that follows it is created). Tags and plain commits give `null`: nothing
 * happens, so a double click never detaches HEAD by accident.
 */
export function pickCheckoutRef(refs: readonly RefInfo[]): RefInfo | null {
  const local = refs.filter((ref) => ref.kind === "local");
  return (
    local.find((ref) => !ref.isHead) ??
    local[0] ??
    refs.find((ref) => ref.kind === "remote") ??
    null
  );
}

/** Where HEAD is compared with the rows in view; `null` when it is in view or not known. */
export function headSide(
  headRow: number | null,
  visible: readonly [number, number] | null,
): "above" | "below" | null {
  if (headRow === null || visible === null) return null;
  if (headRow < visible[0]) return "above";
  if (headRow > visible[1]) return "below";
  return null;
}
