import type { MenuNode } from "./appMenu";

/** One action of the palette: an entry of the menu bar, with the path that leads to it. */
export interface PaletteEntry {
  /** What the application does when it is chosen (see `parseMenuAction`). */
  id: string;
  /** `Branch › New Branch…` */
  path: string;
  accelerator?: string;
}

/** Menus whose entries the palette leaves out: they are about the window, not the repository. */
const SKIPPED_MENUS = new Set(["Window"]);

/**
 * The usable entries of a menu model, flattened, each with its path. Entries that are greyed
 * (and the submenus they would lead to) are left out, so everything listed can be run.
 */
export function paletteEntries(nodes: readonly MenuNode[], trail: string[] = []): PaletteEntry[] {
  const entries: PaletteEntry[] = [];
  for (const node of nodes) {
    if (node.kind === "item") {
      if (!node.enabled) continue;
      entries.push({
        id: node.id,
        path: [...trail, node.label].join(" › "),
        accelerator: node.accelerator,
      });
    } else if (node.kind === "submenu") {
      if (node.enabled === false || (trail.length === 0 && SKIPPED_MENUS.has(node.label))) continue;
      entries.push(...paletteEntries(node.items, [...trail, node.label]));
    }
  }
  return entries;
}

/**
 * How well `query` matches `text`: its letters must appear in order (spaces in the query are
 * ignored); higher is better, `null` for no match. Letters that follow each other, that start
 * a word and that come early score more.
 */
export function fuzzyScore(query: string, text: string): number | null {
  const needle = query.toLowerCase().replace(/\s+/g, "");
  if (needle === "") return 0;
  const hay = text.toLowerCase();
  let score = 0;
  let from = 0;
  let previous = -2;
  for (const letter of needle) {
    const at = hay.indexOf(letter, from);
    if (at === -1) return null;
    const startsWord = at === 0 || /[^a-z0-9]/.test(hay[at - 1]);
    score += 1 + (at === previous + 1 ? 3 : 0) + (startsWord ? 2 : 0) - at * 0.01;
    previous = at;
    from = at + 1;
  }
  return score;
}

/**
 * The entries that match, best first. With an empty query the entries run lately come first
 * (`recent` lists their ids, newest first), then the others in menu order.
 */
export function rankEntries(
  entries: readonly PaletteEntry[],
  query: string,
  recent: readonly string[] = [],
): PaletteEntry[] {
  if (query.trim() === "") {
    const rank = (id: string) => {
      const at = recent.indexOf(id);
      return at === -1 ? Number.MAX_SAFE_INTEGER : at;
    };
    return [...entries].sort((a, b) => rank(a.id) - rank(b.id));
  }
  return entries
    .map((entry, order) => ({ entry, order, score: fuzzyScore(query, entry.path) }))
    .filter(
      (hit): hit is { entry: PaletteEntry; order: number; score: number } => hit.score !== null,
    )
    .sort((a, b) => b.score - a.score || a.order - b.order)
    .map((hit) => hit.entry);
}

/** `CmdOrCtrl+Shift+F` as the user reads it: `⌘⇧F` on a Mac, `Ctrl+Shift+F` elsewhere. */
export function formatAccelerator(accelerator: string, mac: boolean): string {
  const parts = accelerator.split("+");
  const key = parts[parts.length - 1];
  const symbols: Record<string, [string, string]> = {
    CmdOrCtrl: ["⌘", "Ctrl"],
    Shift: ["⇧", "Shift"],
    Alt: ["⌥", "Alt"],
  };
  const mods = parts.slice(0, -1).map((mod) => symbols[mod]?.[mac ? 0 : 1] ?? mod);
  return mac ? [...mods, key].join("") : [...mods, key].join("+");
}

/** The ids of the entries run lately, newest first, without repeats, at most `limit`. */
export function remember(recent: readonly string[], id: string, limit = 5): string[] {
  return [id, ...recent.filter((other) => other !== id)].slice(0, limit);
}
