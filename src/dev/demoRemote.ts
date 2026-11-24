import {
  type AskpassEvent,
  GitCommandError,
  type PullStrategy,
  type PushOptions,
  type RemoteProgressEvent,
  type RemoteResult,
  type WorkingStatus,
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
  let diverged = true;
  let knowsPassword = false;
  let promptCount = 0;

  const emit = (opId: string, line: string) =>
    progressListeners.forEach((listener) => listener({ repoId: hooks.repoId, opId, line }));

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

  async function play(opId: string, op: Running, lines: string[]): Promise<void> {
    for (const line of lines) {
      if (op.cancelled) throw cancelled();
      emit(opId, line);
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

  return {
    remotes: async () => ["origin"],
    remoteUrls: async () => [{ name: "origin", url: "git@ssh.dev.azure.com:v3/acme/Demo/demo-repo" }],
    openExternal: async (url: string) => void window.open(url, "_blank", "noopener"),
    openInEditor: async () => {},
    openInTerminal: async () => {},
    findPullRequest: async () => ({ state: "noToken" as const }),
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
