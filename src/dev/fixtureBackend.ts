import { GitCommandError, type GraphMeta, type GraphRef, type GraphRow, type RefInfo } from "@/lib/api";
import type { RepoBackend } from "@/lib/backend";
import { demoCommitDetail, demoFileDiff } from "./demoDiffs";
import { createDemoMerge } from "./demoMerge";
import { createDemoRemote } from "./demoRemote";
import { createDemoStash } from "./demoStash";
import { createDemoWorktree } from "./demoWorktree";

interface Fixture {
  meta: GraphMeta;
  rows: GraphRow[];
}

/**
 * Stand-in backend for plain-browser development: one recorded repository of invented
 * commits (see `dump_graph_fixture` in the Rust tests). Hidden refs are not applied.
 *
 * `window.__simulateRepoChange()` (in the browser console) adds a commit on top of the
 * current branch and announces the change, like the file watcher of the real backend.
 */
export async function createFixtureBackend(): Promise<RepoBackend> {
  const recorded = (await import("./graph-demo.json")) as unknown as Fixture;
  let data: Fixture = structuredClone({ meta: recorded.meta, rows: recorded.rows });
  const listeners = new Set<(repoId: string) => void>();
  let simulated = 0;

  const refsOf = (fixture: Fixture): RefInfo[] =>
    fixture.meta.refs.map((r) => ({
      name: r.name,
      fullName: r.fullName,
      kind: r.kind,
      target: r.target,
      isHead: r.isHead,
    }));

  const info = {
    id: "demo-repo",
    path: "demo-repo (invented data)",
    name: "demo-repo",
    headBranch: "main",
    isBare: false,
  };

  /** A new commit on top of the current branch: every row moves down by one. */
  function addCommitOnTop(summary?: string): string {
    simulated += 1;
    const old = data;
    const top = old.rows[0];
    const head = old.meta.refs.find((r) => r.isHead);
    const id = `ee${String(simulated).padStart(2, "0")}`.padEnd(40, "0");
    const newTop: GraphRow = {
      row: 0,
      id,
      parents: [top.id],
      summary: summary?.split("\n")[0] ?? `Simulated commit ${simulated}`,
      author: "Alice Fakename",
      time: Math.floor(Date.now() / 1000),
      col: top.col,
      lane: top.lane,
      edges: [],
      refs: head
        ? [
            {
              name: head.name,
              fullName: head.fullName,
              kind: head.kind,
              target: id,
              isHead: head.isHead,
            },
          ]
        : [],
    };
    const rows = old.rows.map((r) => ({
      ...r,
      row: r.row + 1,
      // The head branch moved to the new commit.
      refs: r.refs.filter((ref) => !head || ref.fullName !== head.fullName),
    }));
    const meta: GraphMeta = {
      ...old.meta,
      total: old.meta.total + 1,
      lanes: old.meta.lanes.map((lane) => ({
        ...lane,
        firstRow: lane.id === top.lane ? 0 : lane.firstRow + 1,
        lastRow: lane.lastRow + 1,
      })),
      refs: old.meta.refs.map((r) =>
        r.isHead && head && r.fullName === head.fullName
          ? { ...r, target: id, row: 0 }
          : { ...r, row: r.row + 1 },
      ),
    };
    data = { meta, rows: [newTop, ...rows] };
    return id;
  }

  /** Applies a change to the refs, in both the lists the graph reads, then announces it. */
  function editRefs(change: (refs: GraphRef[]) => GraphRef[]): void {
    const refs = change(data.meta.refs);
    const byRow = new Map<number, GraphRef[]>();
    for (const ref of refs) byRow.set(ref.row, [...(byRow.get(ref.row) ?? []), ref]);
    data = {
      meta: { ...data.meta, refs },
      rows: data.rows.map((row) => ({
        ...row,
        refs: (byRow.get(row.row) ?? []).map(({ name, fullName, kind, target, isHead }) => ({
          name,
          fullName,
          kind,
          target,
          isHead,
        })),
      })),
    };
    info.headBranch = refs.find((r) => r.isHead && r.kind === "local")?.name ?? (null as never);
    setTimeout(() => listeners.forEach((listener) => listener(info.id)), 50);
  }

  const rowOf = (id: string | null): GraphRow => {
    const row = id === null ? data.rows[0] : data.rows.find((r) => r.id === id);
    if (!row) throw new GitCommandError("other", "Unknown commit.");
    return row;
  };
  const nameTaken = (name: string, kind: "local" | "tag") =>
    data.meta.refs.some((r) => r.kind === kind && r.name === name);
  const checkName = (name: string, kind: "local" | "tag") => {
    if (!name || /[\s~^:?*[\\]|\.\.|\.lock$|^-/.test(name)) {
      throw new GitCommandError("invalidName", `'${name}' is not a valid ${kind === "tag" ? "tag" : "branch"} name.`);
    }
    if (nameTaken(name, kind)) {
      throw new GitCommandError("alreadyExists", `A ${kind === "tag" ? "tag" : "branch"} named '${name}' already exists.`);
    }
  };
  const addRef = (kind: "local" | "tag", name: string, row: GraphRow, isHead = false): GraphRef => ({
    name,
    fullName: `${kind === "tag" ? "refs/tags" : "refs/heads"}/${name}`,
    kind,
    target: row.id,
    isHead,
    row: row.row,
    lane: row.lane,
  });
  const moveHead = (to: (refs: GraphRef[]) => GraphRef[]) =>
    editRefs((refs) => to(refs.map((r) => ({ ...r, isHead: false }))));

  const worktreeListeners = new Set<(repoId: string) => void>();
  const activityListeners = new Set<(repoId: string) => void>();
  let readOnlyOn = false;
  let demoMerge: ReturnType<typeof createDemoMerge> | null = null;
  const worktree = createDemoWorktree({
    decorate: (status) => demoMerge?.decorate(status) ?? status,
    headId: () => data.rows[0].id,
    addCommit: (summary) => {
      const id = addCommitOnTop(summary);
      // A commit made in the app moves the branch: the graph refreshes like for any change.
      setTimeout(() => listeners.forEach((listener) => listener(info.id)), 50);
      return id;
    },
    changed: () => activityListeners.forEach((listener) => listener(info.id)),
  });

  demoMerge = createDemoMerge({
    plain: worktree.status,
    headId: () => data.rows[0].id,
    readOnly: () => readOnlyOn,
  });
  const merge = demoMerge;

  const stash = createDemoStash({
    status: worktree.status,
    clearChanges: async () => {
      const status = await worktree.status();
      const paths = status.unstaged.map((e) => e.path);
      if (paths.length > 0) await worktree.discard(paths);
    },
    readOnly: () => readOnlyOn,
  });

  const remote = createDemoRemote({
    repoId: info.id,
    status: worktree.status,
    headBranch: () => info.headBranch,
    deleteRemoteRef: (name, branch) =>
      editRefs((refs) => refs.filter((r) => !(r.kind === "remote" && r.name === `${name}/${branch}`))),
  });

  if (import.meta.env.DEV) {
    const win = window as unknown as {
      __simulateRepoChange: () => void;
      __simulateWorktreeChange: () => void;
    };
    win.__simulateRepoChange = () => {
      addCommitOnTop();
      listeners.forEach((listener) => listener(info.id));
    };
    win.__simulateWorktreeChange = () => {
      worktree.touch();
      worktreeListeners.forEach((listener) => listener(info.id));
    };
  }

  return {
    pickFolder: async () => info.path,
    open: async () => ({ info, refs: refsOf(data) }),
    close: async () => {},
    refs: async () => refsOf(data),
    locate: async (_repoId, ids) => ids.map((id) => data.rows.find((r) => r.id === id)?.row ?? null),
    status: worktree.status,
    workingDiff: (_repoId, entry, area, force) => worktree.workingDiff(entry, area, force),
    stage: (_repoId, paths) => worktree.stage(paths),
    unstage: (_repoId, paths) => worktree.unstage(paths),
    stageHunk: (_repoId, path, hunkId) => worktree.stageHunk(path, hunkId),
    unstageHunk: (_repoId, path, hunkId) => worktree.unstageHunk(path, hunkId),
    discard: (_repoId, paths) => worktree.discard(paths),
    snapshots: worktree.snapshots,
    createBranch: async (_repoId, name, start, checkout) => {
      checkName(name, "local");
      const row = rowOf(start);
      moveHead((refs) => [...refs, addRef("local", name, row, checkout)].map((r) =>
        checkout ? r : { ...r, isHead: r.isHead || (r.kind === "local" && r.name === info.headBranch) },
      ));
      return await worktree.status();
    },
    checkout: async (_repoId, target, strategy) => {
      const dirty = (await worktree.status()).unstaged.map((e) => e.path);
      // The demo pretends that the first changed file is in the way of any switch.
      if (dirty.length > 0 && strategy === null) {
        throw new GitCommandError(
          "wouldOverwriteChanges",
          "Your local changes would be overwritten by checkout.",
          dirty.slice(0, 1),
        );
      }
      const refs = data.meta.refs;
      if (target.kind === "commit") {
        rowOf(target.id);
        moveHead((all) => all);
      } else {
        const found = refs.find(
          (r) => r.name === target.name && r.kind === (target.kind === "branch" ? "local" : target.kind),
        );
        if (!found) throw new GitCommandError("other", `Unknown ref '${target.name}'.`);
        if (target.kind === "branch") {
          moveHead((all) => all.map((r) => (r.fullName === found.fullName ? { ...r, isHead: true } : r)));
        } else if (target.kind === "remote") {
          const local = target.name.replace(/^[^/]+\//, "");
          const existing = refs.find((r) => r.kind === "local" && r.name === local);
          moveHead((all) =>
            existing
              ? all.map((r) => (r.fullName === existing.fullName ? { ...r, isHead: true } : r))
              : [...all, { ...addRef("local", local, rowOf(found.target), true) }],
          );
        } else {
          moveHead((all) => all);
        }
      }
      return { status: await worktree.status(), snapshot: null, stashed: strategy === "stash", stashConflict: false };
    },
    renameBranch: async (_repoId, oldName, newName) => {
      checkName(newName, "local");
      editRefs((refs) =>
        refs.map((r) =>
          r.kind === "local" && r.name === oldName
            ? { ...r, name: newName, fullName: `refs/heads/${newName}` }
            : r,
        ),
      );
      return await worktree.status();
    },
    deleteBranch: async (_repoId, name, force) => {
      const found = data.meta.refs.find((r) => r.kind === "local" && r.name === name);
      if (!found) throw new GitCommandError("other", `Unknown branch '${name}'.`);
      if (found.isHead) throw new GitCommandError("branchCheckedOut", `Cannot delete branch '${name}': it is checked out.`);
      const alone = !data.meta.refs.some((r) => r !== found && r.target === found.target);
      if (alone && !force && found.row > 2) {
        throw new GitCommandError("notFullyMerged", `The branch '${name}' is not fully merged.`);
      }
      editRefs((refs) => refs.filter((r) => r !== found));
      return { name, tip: found.target, upstream: null, status: await worktree.status() };
    },
    restoreBranch: async (_repoId, name, tip) => {
      checkName(name, "local");
      editRefs((refs) => [...refs, addRef("local", name, rowOf(tip))]);
      return await worktree.status();
    },
    setUpstream: async () => worktree.status(),
    statusHint: async () => ({ lastMs: 35, untrackedCache: false, canSpeedUp: false }),
    speedUpStatus: async () => ({ lastMs: 20, untrackedCache: true, canSpeedUp: false }),
    ...remote,
    ...stash,
    mergeInfo: merge.mergeInfo,
    merge: merge.merge,
    conflictFile: merge.conflictFile,
    resolveFile: merge.resolveFile,
    resolveSide: merge.resolveSide,
    continueOperation: merge.continueOperation,
    abortOperation: merge.abortOperation,
    rebasePlan: merge.rebasePlan,
    rebase: merge.rebase,
    skipOperation: merge.skipOperation,
    createTag: async (_repoId, name, target) => {
      checkName(name, "tag");
      editRefs((refs) => [...refs, addRef("tag", name, rowOf(target))]);
      return await worktree.status();
    },
    deleteTag: async (_repoId, name) => {
      const found = data.meta.refs.find((r) => r.kind === "tag" && r.name === name);
      if (!found) throw new GitCommandError("other", `Unknown tag '${name}'.`);
      editRefs((refs) => refs.filter((r) => r !== found));
      return { name, target: found.target, status: await worktree.status() };
    },
    restoreTag: async (_repoId, name, target) => {
      editRefs((refs) => [...refs, addRef("tag", name, rowOf(target))]);
      return await worktree.status();
    },
    restoreSnapshot: (_repoId, reference) => worktree.restoreSnapshot(reference),
    commit: (_repoId, summary, body, amend) => worktree.commit(summary, body, amend),
    undoCommit: (_repoId, before, after) => worktree.undoCommit(before, after),
    identity: worktree.identity,
    setIdentity: (_repoId, name, email) => worktree.setIdentity(name, email),
    setReadOnly: (_repoId, readOnly) => {
      readOnlyOn = readOnly;
      return worktree.setReadOnly(readOnly);
    },
    activity: worktree.activity,
    clearActivity: worktree.clearActivity,
    onWorktreeChanged: async (handler) => {
      worktreeListeners.add(handler);
      return () => void worktreeListeners.delete(handler);
    },
    onActivityChanged: async (handler) => {
      activityListeners.add(handler);
      return () => void activityListeners.delete(handler);
    },
    onRepoChanged: async (handler) => {
      listeners.add(handler);
      return () => void listeners.delete(handler);
    },
    search: async (_repoId, query, limit) => {
      const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
      if (terms.length === 0) return { rows: [], total: 0 };
      const rows = data.rows
        .filter((r) =>
          terms.every(
            (t) =>
              r.summary.toLowerCase().includes(t) ||
              r.author.toLowerCase().includes(t) ||
              r.id.startsWith(t),
          ),
        )
        .map((r) => r.row);
      return { rows: rows.slice(0, limit), total: rows.length };
    },
    commitDetail: async (_repoId, commitId) => demoCommitDetail(commitId),
    fileDiff: async (_repoId, _commitId, file, force) => demoFileDiff(file, force),
    graph: async () => {
      const current = data;
      return {
        meta: current.meta,
        fetchPage: async (cursor, limit) => {
          const end = Math.min(current.rows.length, cursor + limit);
          return {
            start: cursor,
            rows: current.rows.slice(cursor, end),
            nextCursor: end < current.rows.length ? end : null,
            total: current.rows.length,
          };
        },
      };
    },
  };
}
