import type { RefInfo } from "@/lib/api";

/** One remote that has the branch. */
export interface RemoteCopy {
  remote: string;
  ref: RefInfo;
}

/**
 * The refs of a commit that are the same branch, in one label: `feature/x`, `origin/feature/x`
 * and `upstream/feature/x` make one group, shown with an icon per place the branch lives in.
 * A tag is a group of its own.
 */
export interface RefGroup {
  /** The name shown: the branch name without the remote, or the tag name. */
  name: string;
  local: RefInfo | null;
  remotes: RemoteCopy[];
  tag: RefInfo | null;
  isHead: boolean;
  /** The ref a click, a double click or a context menu acts on: the local one first. */
  primary: RefInfo;
}

/** The remote a remote-tracking ref belongs to (`origin` for `origin/feature/x`). */
export function remoteOf(ref: RefInfo): string {
  const prefix = "refs/remotes/";
  const rest = ref.fullName.startsWith(prefix) ? ref.fullName.slice(prefix.length) : ref.name;
  return rest.split("/")[0];
}

/** The branch name of a remote-tracking ref (`feature/x` for `origin/feature/x`). */
function branchOf(ref: RefInfo): string {
  const slash = ref.name.indexOf("/");
  return slash < 0 ? ref.name : ref.name.slice(slash + 1);
}

/** Groups the refs of one commit; the current branch first, then local, remote, tags. */
export function groupRefs(refs: readonly RefInfo[]): RefGroup[] {
  const groups = new Map<string, RefGroup>();
  const order: RefGroup[] = [];
  const add = (key: string, make: () => RefGroup): RefGroup => {
    let group = groups.get(key);
    if (!group) {
      group = make();
      groups.set(key, group);
      order.push(group);
    }
    return group;
  };
  for (const ref of refs) {
    if (ref.kind === "tag") {
      add(`tag:${ref.name}`, () => ({
        name: ref.name,
        local: null,
        remotes: [],
        tag: ref,
        isHead: false,
        primary: ref,
      }));
      continue;
    }
    const name = ref.kind === "local" ? ref.name : branchOf(ref);
    // `origin/HEAD` only says which branch is the default one.
    if (ref.kind === "remote" && name === "HEAD") continue;
    const group = add(`branch:${name}`, () => ({
      name,
      local: null,
      remotes: [],
      tag: null,
      isHead: false,
      primary: ref,
    }));
    if (ref.kind === "local") {
      group.local = ref;
      group.primary = ref;
      group.isHead = group.isHead || ref.isHead;
    } else {
      group.remotes.push({ remote: remoteOf(ref), ref });
    }
  }
  const rank = (g: RefGroup) => (g.isHead ? 0 : g.local ? 1 : g.tag ? 3 : 2);
  return order
    .map((group, index) => ({ group, index }))
    .sort((a, b) => rank(a.group) - rank(b.group) || a.index - b.index)
    .map(({ group }) => group);
}
