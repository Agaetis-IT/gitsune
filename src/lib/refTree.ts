import type { RefInfo } from "@/lib/api";

/** A row of the branch list: a folder (the part of a name before a `/`) or a ref in it. */
export type RefRow =
  | {
      kind: "folder";
      /** Full path of the folder (`feature/auth`), unique inside one group. */
      path: string;
      name: string;
      depth: number;
      collapsed: boolean;
      /** Every ref below it, at any depth. */
      refs: RefInfo[];
      /** HEAD is inside (worth showing when the folder is closed). */
      hasHead: boolean;
    }
  | {
      kind: "ref";
      ref: RefInfo;
      /** The last part of the name: `auth-18` for `feature/auth-18`. */
      label: string;
      depth: number;
    };

interface Node {
  name: string;
  path: string;
  folders: Map<string, Node>;
  refs: Array<{ ref: RefInfo; label: string }>;
  all: RefInfo[];
}

const node = (name: string, path: string): Node => ({
  name,
  path,
  folders: new Map(),
  refs: [],
  all: [],
});

const byName = (a: string, b: string) => a.localeCompare(b, undefined, { numeric: true });

/** Puts refs in folders by the `/` of their names. A name never ends in a folder of its own. */
export function buildTree(refs: readonly RefInfo[]): Node {
  const root = node("", "");
  for (const ref of refs) {
    const parts = ref.name.split("/").filter((part) => part !== "");
    if (parts.length === 0) continue;
    let current = root;
    current.all.push(ref);
    for (const part of parts.slice(0, -1)) {
      const path = current.path ? `${current.path}/${part}` : part;
      let next = current.folders.get(part);
      if (!next) {
        next = node(part, path);
        current.folders.set(part, next);
      }
      current = next;
      current.all.push(ref);
    }
    current.refs.push({ ref, label: parts[parts.length - 1] });
  }
  return root;
}

/**
 * The visible rows of a group, depth first: folders (open or closed) before the refs of the
 * same level, each sorted by name. `collapsed` holds the paths of the folders that are closed.
 */
export function buildRows(refs: readonly RefInfo[], collapsed: ReadonlySet<string>): RefRow[] {
  const rows: RefRow[] = [];
  const walk = (parent: Node, depth: number) => {
    const folders = [...parent.folders.values()].sort((a, b) => byName(a.name, b.name));
    for (const folder of folders) {
      const closed = collapsed.has(folder.path);
      rows.push({
        kind: "folder",
        path: folder.path,
        name: folder.name,
        depth,
        collapsed: closed,
        refs: folder.all,
        hasHead: folder.all.some((ref) => ref.isHead),
      });
      if (!closed) walk(folder, depth + 1);
    }
    const leaves = [...parent.refs].sort((a, b) => byName(a.label, b.label));
    for (const leaf of leaves) rows.push({ kind: "ref", ref: leaf.ref, label: leaf.label, depth });
  };
  walk(buildTree(refs), 0);
  return rows;
}

/** The paths of every folder of a group (to open or close them all). */
export function folderPaths(refs: readonly RefInfo[]): string[] {
  const paths: string[] = [];
  const walk = (parent: Node) => {
    for (const folder of parent.folders.values()) {
      paths.push(folder.path);
      walk(folder);
    }
  };
  walk(buildTree(refs));
  return paths;
}
