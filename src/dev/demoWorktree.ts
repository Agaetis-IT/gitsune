import {
  type ActivityEntry,
  type Area,
  type CommitResult,
  type DiffHunk,
  type DiscardResult,
  type FileDiff,
  type FileState,
  GitCommandError,
  type Identity,
  type Snapshot,
  type StatusEntry,
  type WorkingStatus,
} from "@/lib/api";
import { hunk } from "./demoDiffs";

/** An in-memory working tree of invented changes, so the staging and commit flows can be tried
 * without Tauri. Magic words in a commit summary trigger the failures: `hook`, `sign`, `ident`. */

interface DemoChange {
  path: string;
  oldPath: string | null;
  /** What the change is as a whole, before anything is staged. */
  state: FileState;
  unstaged: DiffHunk[];
  staged: DiffHunk[];
  binary: boolean;
}

const lines = (...text: string[]) => text.join("\n");

function initialChanges(): DemoChange[] {
  const change = (c: Partial<DemoChange> & Pick<DemoChange, "path" | "state">): DemoChange => ({
    oldPath: null,
    unstaged: [],
    staged: [],
    binary: false,
    ...c,
  });
  return [
    change({
      path: "src/parser/lexer.ts",
      state: "modified",
      unstaged: [
        hunk(
          10,
          10,
          lines(
            " export class Lexer {",
            "   private position = 0;",
            "-  private line = 1;",
            "+  private line = 1;",
            "+  private column = 0;",
            " ",
            "   constructor(private readonly source: string) {}",
          ),
        ),
        hunk(
          42,
          43,
          lines(
            "   next(): Token | null {",
            "-    const char = this.source[this.position++];",
            "+    const char = this.source[this.position];",
            "     if (char === undefined) return null;",
            "+    this.position += 1;",
            "+    this.column += 1;",
            "     return { kind: classify(char), text: char, line: this.line };",
            "   }",
          ),
        ),
      ],
    }),
    change({
      path: "src/ui/ActionButton.vue",
      oldPath: "src/ui/Button.vue",
      state: "renamed",
      staged: [
        hunk(
          3,
          3,
          lines(
            " <template>",
            '-  <button class="btn"><slot /></button>',
            '+  <button class="btn" type="button"><slot /></button>',
            " </template>",
          ),
        ),
      ],
      unstaged: [
        hunk(
          9,
          9,
          lines(" <style scoped>", " .btn {", "-  padding: 4px;", "+  padding: 6px 10px;", " }"),
        ),
      ],
    }),
    change({
      path: "README.md",
      state: "modified",
      unstaged: [
        hunk(1, 1, lines("-# Parser demo", "+# Parser demo (invented data)", " ", " A small project.")),
      ],
    }),
    change({
      path: "docs/old-notes.md",
      state: "deleted",
      unstaged: [hunk(1, 0, lines("-# Old notes", "-", "-Things we tried before the rewrite."))],
    }),
    change({
      path: "notes/todo.md",
      state: "untracked",
      unstaged: [hunk(0, 1, lines("+# To do", "+", "+- write more tests", "+- try the staging flow"))],
    }),
    change({ path: "assets/logo.png", state: "modified", binary: true, unstaged: [] }),
    change({
      path: "src/parser/tokens.ts",
      state: "added",
      staged: [
        hunk(
          0,
          1,
          lines(
            "+export type TokenKind = 'word' | 'number' | 'symbol';",
            "+",
            "+export interface Token {",
            "+  kind: TokenKind;",
            "+  text: string;",
            "+}",
          ),
        ),
      ],
    }),
  ];
}

export interface DemoWorktreeDeps {
  /** The commit HEAD points at. */
  headId: () => string;
  /** Adds a commit on top of the demo graph and returns its id. */
  addCommit: (summary: string) => string;
  /** Tells the app that the working tree or the activity changed. */
  changed: () => void;
  /** Adds what the demo merge knows (operation in progress, conflicts) to every status. */
  decorate?: (status: WorkingStatus) => WorkingStatus;
}

export function createDemoWorktree(deps: DemoWorktreeDeps) {
  let changes = initialChanges();
  let identitySet = false;
  let readOnly = false;
  let activityId = 0;
  const activity: ActivityEntry[] = [];
  const snapshots: Snapshot[] = [];
  const saved = new Map<string, DemoChange[]>(); // snapshot reference -> discarded changes
  const committed = new Map<string, DemoChange[]>(); // commit id -> what was staged

  const clone = <T>(value: T): T => structuredClone(value);

  function log(command: string, ok = true, stderr = ""): void {
    activity.push({
      id: ++activityId,
      startedAt: Date.now(),
      command,
      durationMs: 3 + Math.round(Math.random() * 12),
      exitCode: ok ? 0 : 1,
      ok,
      stdout: "",
      stderr,
      truncated: false,
    });
    deps.changed();
  }

  function guard(): void {
    if (readOnly) throw new GitCommandError("readOnly", "This repository is in read-only mode.");
  }

  function status(): WorkingStatus {
    const staged: StatusEntry[] = [];
    const unstaged: StatusEntry[] = [];
    for (const c of changes) {
      const stagedHere = c.staged.length > 0 || (c.binary && c.state === "added");
      if (stagedHere) {
        staged.push({
          path: c.path,
          oldPath: c.oldPath,
          state: c.state === "untracked" ? "added" : c.state,
          submodule: false,
        });
      }
      if (c.unstaged.length > 0 || (c.binary && !stagedHere)) {
        unstaged.push({
          path: c.path,
          oldPath: null,
          state: c.state === "renamed" ? "modified" : c.state,
          submodule: false,
        });
      }
    }
    const byPath = (a: StatusEntry, b: StatusEntry) => a.path.localeCompare(b.path);
    const plain: WorkingStatus = {
      operation: null,
      branch: "main",
      detached: false,
      head: deps.headId(),
      upstream: "origin/main",
      ahead: 1,
      behind: 0,
      staged: staged.sort(byPath),
      unstaged: unstaged.sort(byPath),
      conflicted: [],
    };
    return deps.decorate ? deps.decorate(plain) : plain;
  }

  const find = (path: string) => changes.find((c) => c.path === path);

  function dropEmpty(): void {
    changes = changes.filter(
      (c) => c.staged.length > 0 || c.unstaged.length > 0 || c.binary || c.state === "deleted",
    );
  }

  function stagePath(c: DemoChange): void {
    c.staged.push(...c.unstaged);
    c.unstaged = [];
    if (c.binary) c.state = c.state === "modified" ? "modified" : c.state;
  }

  return {
    status: async () => status(),
    setReadOnly: async (value: boolean) => {
      readOnly = value;
    },
    identity: async (): Promise<Identity> =>
      identitySet
        ? { name: "Alice Fakename", email: "alice@example.invalid" }
        : { name: null, email: null },
    setIdentity: async (name: string, email: string): Promise<Identity> => {
      guard();
      identitySet = true;
      log(`git config --local user.name '${name}'`);
      return { name, email };
    },
    activity: async () => [...activity],
    clearActivity: async () => {
      activity.length = 0;
    },

    stage: async (paths: string[]) => {
      guard();
      for (const path of paths) {
        const c = find(path);
        if (c) stagePath(c);
      }
      log(`git add -A -- ${paths.join(" ")}`);
      return status();
    },
    unstage: async (paths: string[]) => {
      guard();
      for (const path of paths) {
        const c = find(path);
        if (c) {
          c.unstaged.push(...c.staged);
          c.staged = [];
          if (c.state === "added") c.state = "untracked";
        }
      }
      log(`git restore --staged -- ${paths.join(" ")}`);
      return status();
    },
    stageHunk: async (path: string, hunkId: string) => {
      guard();
      const c = find(path);
      const at = c?.unstaged.findIndex((h) => h.id === hunkId) ?? -1;
      if (!c || at === -1) {
        throw new GitCommandError("staleHunk", "The file changed since this hunk was shown.");
      }
      c.staged.push(...c.unstaged.splice(at, 1));
      log("git apply --cached --recount - < [patch]");
      return status();
    },
    unstageHunk: async (path: string, hunkId: string) => {
      guard();
      const c = find(path);
      const at = c?.staged.findIndex((h) => h.id === hunkId) ?? -1;
      if (!c || at === -1) {
        throw new GitCommandError("staleHunk", "The file changed since this hunk was shown.");
      }
      c.unstaged.push(...c.staged.splice(at, 1));
      log("git apply --cached --recount --reverse - < [patch]");
      return status();
    },

    workingDiff: async (entry: StatusEntry, area: Area, force: boolean): Promise<FileDiff> => {
      const c = find(entry.path);
      const hunks = (area === "staged" ? c?.staged : c?.unstaged) ?? [];
      const count = (kind: "add" | "delete") =>
        hunks.reduce((n, h) => n + h.lines.filter((l) => l.kind === kind).length, 0);
      return {
        path: entry.path,
        oldPath: area === "staged" ? entry.oldPath : null,
        status:
          entry.state === "untracked" || entry.state === "added"
            ? "added"
            : entry.state === "deleted"
              ? "deleted"
              : entry.state === "renamed"
                ? "renamed"
                : "modified",
        binary: !!c?.binary,
        submodule: false,
        tooLarge: false,
        canForce: false,
        truncated: false,
        oldSize: null,
        newSize: null,
        additions: count("add"),
        deletions: count("delete"),
        hunks: force || !c?.binary ? hunks : [],
      };
    },

    discard: async (paths: string[]): Promise<DiscardResult> => {
      guard();
      const taken: DemoChange[] = [];
      for (const path of paths) {
        const c = find(path);
        if (!c || (c.unstaged.length === 0 && !c.binary)) continue;
        taken.push(clone({ ...c, staged: [] }));
        c.unstaged = [];
        if (c.state === "untracked") c.binary = false;
        if (c.binary) c.binary = false;
      }
      dropEmpty();
      log(`git restore -- ${paths.join(" ")}`);
      if (taken.length === 0) return { snapshot: null, status: status() };
      const reference = `refs/gitsune/undo/${Date.now()}-${snapshots.length}`;
      const snapshot: Snapshot = {
        reference,
        id: reference.slice(-12).padEnd(40, "0"),
        time: Math.floor(Date.now() / 1000),
        description: "before discarding changes",
        paths: taken.map((c) => c.path),
      };
      snapshots.unshift(snapshot);
      saved.set(reference, taken);
      return { snapshot, status: status() };
    },
    snapshots: async () => [...snapshots],
    restoreSnapshot: async (reference: string): Promise<DiscardResult> => {
      guard();
      const taken = saved.get(reference);
      if (!taken) throw new GitCommandError("other", "That snapshot no longer exists.");
      for (const back of taken) {
        const existing = find(back.path);
        if (existing) existing.unstaged = back.unstaged;
        else changes.push(back);
      }
      saved.delete(reference);
      const at = snapshots.findIndex((s) => s.reference === reference);
      if (at !== -1) snapshots.splice(at, 1);
      log(`git restore --source=<snapshot> --worktree -- ${taken.map((c) => c.path).join(" ")}`);
      return { snapshot: null, status: status() };
    },

    commit: async (summary: string, body: string, amend: boolean): Promise<CommitResult> => {
      guard();
      const before = deps.headId();
      const lower = summary.toLowerCase();
      const fail = (kind: ConstructorParameters<typeof GitCommandError>[0], text: string) => {
        log("git commit -F - < [message]", false, text);
        throw new GitCommandError(kind, text);
      };
      if (lower.includes("ident") && !identitySet) {
        fail("needsIdentity", "Author identity unknown\n\n*** Please tell me who you are.");
      }
      if (lower.includes("hook")) {
        fail("hookFailed", "pre-commit: lint failed\n  src/parser/lexer.ts:12  unused variable 'column'");
      }
      if (lower.includes("sign")) fail("signingFailed", "error: gpg failed to sign the data");
      const hasStaged = changes.some((c) => c.staged.length > 0);
      if (!hasStaged && !amend) fail("nothingToCommit", "nothing to commit, working tree clean");
      const stagedNow = clone(changes.filter((c) => c.staged.length > 0));
      for (const c of changes) c.staged = [];
      dropEmpty();
      const id = deps.addCommit(body ? `${summary}\n\n${body}` : summary);
      committed.set(id, stagedNow);
      log(`git commit -F -${amend ? " --amend" : ""} < [message]`);
      return { id, before, status: status() };
    },
    undoCommit: async (_before: string | null, after: string): Promise<WorkingStatus> => {
      guard();
      for (const back of committed.get(after) ?? []) {
        const existing = find(back.path);
        if (existing) existing.staged = back.staged;
        else changes.push(back);
      }
      committed.delete(after);
      log("git reset --soft <previous commit>");
      return status();
    },

    /** Dev helper: a new file appears in the working tree, as if saved from an editor. */
    touch: (): void => {
      const n = changes.filter((c) => c.path.startsWith("src/extra")).length + 1;
      changes.push({
        path: `src/extra-${n}.ts`,
        oldPath: null,
        state: "untracked",
        binary: false,
        staged: [],
        unstaged: [hunk(0, 1, lines(`+export const extra${n} = ${n};`))],
      });
    },
  };
}
