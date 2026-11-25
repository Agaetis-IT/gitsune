import {
  type AskpassEvent,
  GitCommandError,
  type PullStrategy,
  type PushOptions,
  type RemoteProgressEvent,
  type RemoteResult,
  type WorkingStatus,
  type Worktree,
} from "@/lib/api";

interface Hooks {
  repoId: string;
  status: () => Promise<WorkingStatus>;
  headBranch: () => string | null;
  /** Removes a remote-tracking ref from the demo graph. */
  deleteRemoteRef: (remote: string, branch: string) => void;
}

interface Running {
  cancelled: boolean;
  /** Set while a question waits for the user. */
  answer: ((value: string | null) => void) | null;
}

const wait = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

/**
 * Stand-in for fetch, pull and push in the plain browser: progress lines, one password
 * question the first time something is pushed, a pull that needs a strategy the first time,
 * and a first push that needs an upstream. It exercises every dialog of the interface.
 */
export function createDemoRemote(hooks: Hooks) {
  const progressListeners = new Set<(event: RemoteProgressEvent) => void>();
  const askListeners = new Set<(event: AskpassEvent) => void>();
  const running = new Map<string, Running>();
  const pushed = new Set<string>();
  let remoteList: Array<{ name: string; url: string }> = [
    { name: "origin", url: "git@ssh.dev.azure.com:v3/acme/Demo/demo-repo" },
  ];
  let diverged = true;
  let knowsPassword = false;
  let promptCount = 0;

  const emit = (opId: string, line: string, owner = hooks.repoId) =>
    progressListeners.forEach((listener) => listener({ repoId: owner, opId, line }));

  async function track<T>(opId: string, work: (op: Running) => Promise<T>): Promise<T> {
    const op: Running = { cancelled: false, answer: null };
    running.set(opId, op);
    try {
      return await work(op);
    } finally {
      running.delete(opId);
    }
  }

  const cancelled = () => new GitCommandError("cancelled", "Cancelled.");

  async function play(
    opId: string,
    op: Running,
    lines: string[],
    owner = hooks.repoId,
  ): Promise<void> {
    for (const line of lines) {
      if (op.cancelled) throw cancelled();
      emit(opId, line, owner);
      await wait(180);
    }
    if (op.cancelled) throw cancelled();
  }

  const transfer = (verb: string) => [
    "remote: Enumerating objects: 24, done.",
    `${verb} objects:  20% (5/24)`,
    `${verb} objects:  60% (15/24)`,
    `${verb} objects: 100% (24/24), 3.1 KiB | 1.5 MiB/s, done.`,
  ];

  async function ask(opId: string, op: Running, prompt: string): Promise<string> {
    const answer = await new Promise<string | null>((resolve) => {
      op.answer = resolve;
      askListeners.forEach((listener) =>
        listener({ repoId: hooks.repoId, opId, promptId: String(++promptCount), prompt }),
      );
    });
    op.answer = null;
    if (op.cancelled) throw cancelled();
    if (answer === null) {
      throw new GitCommandError("authFailed", "fatal: Authentication failed for the remote.");
    }
    return answer;
  }

  async function done(summary: string): Promise<RemoteResult> {
    return { status: await hooks.status(), summary };
  }

  // A main working tree and one more, so the sidebar group and its dialogs can be tried.
  let trees: Worktree[] = [
    {
      path: "/work/demo-repo",
      head: "a".repeat(40),
      branch: "main",
      detached: false,
      bare: false,
      isMain: true,
      locked: null,
      prunable: null,
      dirty: false,
    },
    {
      path: "/work/demo-repo-search",
      head: "b".repeat(40),
      branch: "feature/api-15",
      detached: false,
      bare: false,
      isMain: false,
      locked: null,
      prunable: null,
      dirty: true,
    },
  ];

  return {
    worktrees: async () => structuredClone(trees),
    addWorktree: async (_repoId: string, path: string, branch: string) => {
      trees = [
        ...trees,
        {
          path,
          head: "c".repeat(40),
          branch,
          detached: false,
          bare: false,
          isMain: false,
          locked: null,
          prunable: null,
          dirty: false,
        },
      ];
    },
    removeWorktree: async (_repoId: string, path: string, force: boolean) => {
      const tree = trees.find((t) => t.path === path);
      if (tree?.dirty && !force) {
        throw new GitCommandError(
          "other",
          `fatal: '${path}' contains modified or untracked files, use --force to delete it`,
        );
      }
      trees = trees.filter((t) => t.path !== path);
    },
    lockWorktree: async (_repoId: string, path: string, lock: boolean) => {
      trees = trees.map((t) => (t.path === path ? { ...t, locked: lock ? "" : null } : t));
    },
    pruneWorktrees: async () => {
      trees = trees.filter((t) => t.prunable === null);
    },
    remotes: async () => remoteList.map((remote) => remote.name),
    remoteUrls: async () => remoteList,
    addRemote: async (_repoId: string, name: string, url: string) => {
      if (remoteList.some((remote) => remote.name === name)) {
        throw new GitCommandError("alreadyExists", `A remote named '${name}' already exists.`);
      }
      remoteList = [...remoteList, { name, url }];
      return remoteList;
    },
    renameRemote: async (_repoId: string, name: string, newName: string) => {
      if (name !== newName && remoteList.some((remote) => remote.name === newName)) {
        throw new GitCommandError("alreadyExists", `A remote named '${newName}' already exists.`);
      }
      remoteList = remoteList.map((r) => (r.name === name ? { ...r, name: newName } : r));
      return remoteList;
    },
    setRemoteUrl: async (_repoId: string, name: string, url: string) => {
      remoteList = remoteList.map((r) => (r.name === name ? { ...r, url } : r));
      return remoteList;
    },
    removeRemote: async (_repoId: string, name: string) => {
      remoteList = remoteList.filter((remote) => remote.name !== name);
      return remoteList;
    },
    pushTags: (_repoId: string, opId: string, _remote: string, tag: string | null) =>
      track(opId, async (op) => {
        await play(opId, op, ["Enumerating objects: 1, done.", "Writing objects: 100% (1/1), done."]);
        return done(`To example.invalid:demo/repo.git\n * [new tag]  ${tag ?? "v1.0"} -> ${tag ?? "v1.0"}`);
      }),
    openExternal: async (url: string) => void window.open(url, "_blank", "noopener"),
    openInEditor: async () => {},
    openInTerminal: async () => {},
    findPullRequest: async () => ({ state: "signedOut" as const }),
    defaultBranch: async () => "main" as string | null,
    createPullRequest: async () => 99,
    autoFetch: async () => ({
      behindTotal: 3,
      updates: [{ branch: "main", upstream: "origin/main", newCommits: 3 }],
    }),
    authors: async () => [
      { name: "Dan Testuser", count: 40 },
      { name: "Eve Example", count: 12 },
      { name: "Sam Sample", count: 7 },
    ],
    commitsBetween: async () => [
      { id: "a".repeat(40), summary: "Add the search overlay" },
      { id: "b".repeat(40), summary: "Fix the overlay focus" },
    ],
    listPullRequests: async () => ({
      state: "listed" as const,
      items: [
        { branch: "feature/api-10", id: 41, title: "Rework the API", draft: false, review: "approved" as const, checks: "success" as const },
        { branch: "feature/auth-16", id: 42, title: "Auth refresh", draft: false, review: "none" as const, checks: "failure" as const },
        { branch: "feature/auth-18", id: 43, title: "Auth cleanup", draft: true, review: "none" as const, checks: "pending" as const },
        { branch: "feature/docs-1", id: 44, title: "Docs", draft: false, review: "changesRequested" as const, checks: "none" as const },
        { branch: "main", id: 45, title: "Release", draft: false, review: "none" as const, checks: "none" as const },
      ],
    }),
    fetch: (_repoId: string, opId: string) =>
      track(opId, async (op) => {
        await play(opId, op, transfer("Receiving"));
        return done("From example.invalid:demo/repo\n   1a2b3c4..5d6e7f8  main -> origin/main");
      }),
    pull: (_repoId: string, opId: string, strategy: PullStrategy | null) =>
      track(opId, async (op) => {
        await play(opId, op, transfer("Receiving"));
        if (diverged && strategy === null) {
          throw new GitCommandError(
            "divergedBranches",
            "fatal: Need to specify how to reconcile divergent branches.",
          );
        }
        const wasDiverged = diverged;
        diverged = false;
        return done(wasDiverged ? "Merge made by the 'ort' strategy." : "Already up to date.");
      }),
    push: (_repoId: string, opId: string, options: PushOptions) =>
      track(opId, async (op) => {
        const branch = options.branch ?? hooks.headBranch() ?? "main";
        if (!options.remote && !pushed.has(branch)) {
          throw new GitCommandError(
            "noUpstream",
            `fatal: The current branch ${branch} has no upstream branch.`,
          );
        }
        if (!knowsPassword) {
          await ask(opId, op, "Password for 'https://demo@example.invalid': ");
          knowsPassword = true;
        }
        await play(opId, op, [
          "Enumerating objects: 5, done.",
          "Writing objects:  50% (2/4)",
          "Writing objects: 100% (4/4), 412 bytes | 412.00 KiB/s, done.",
        ]);
        pushed.add(branch);
        return done(`To example.invalid:demo/repo.git\n   5d6e7f8..9a0b1c2  ${branch} -> ${branch}`);
      }),
    cloneRepository: (ownerId: string, opId: string, _url: string, parent: string, name: string) =>
      track(opId, async (op) => {
        await play(opId, op, [`Cloning into '${name}'...`, ...transfer("Receiving")], ownerId);
        return `${parent}/${name}`;
      }),
    deleteRemoteBranch: (_repoId: string, opId: string, remote: string, branch: string) =>
      track(opId, async (op) => {
        await play(opId, op, ["Connecting to the remote…"]);
        hooks.deleteRemoteRef(remote, branch);
        return done(`To example.invalid:demo/repo.git\n - [deleted]  ${branch}`);
      }),
    cancelOperation: async (opId: string) => {
      const op = running.get(opId);
      if (!op) return;
      op.cancelled = true;
      op.answer?.(null);
    },
    answerAskpass: async (opId: string, _promptId: string, answer: string | null) => {
      running.get(opId)?.answer?.(answer);
    },
    onRemoteProgress: async (handler: (event: RemoteProgressEvent) => void) => {
      progressListeners.add(handler);
      return () => void progressListeners.delete(handler);
    },
    onAskpass: async (handler: (event: AskpassEvent) => void) => {
      askListeners.add(handler);
      return () => void askListeners.delete(handler);
    },
  };
}
