import type { RefInfo, Worktree } from "@/lib/api";
import { ticketFromBranch } from "@/lib/tickets";

/** One line of a context menu. A disabled item says why (shown as a tooltip and in the menu). */
export interface MenuItem {
  id: string;
  label: string;
  /** The reason the action is not available right now. */
  disabled?: string;
  /** A destructive action. */
  danger?: boolean;
  /** Draw a separator above this item. */
  separatorBefore?: boolean;
}

/** What the menus need to know to tell which actions make sense. */
export interface MenuContext {
  readOnly: boolean;
  /** The commit HEAD points at. */
  headId: string | null;
  detached: boolean;
  /** The branch of a lane or ref can be highlighted. */
  canHighlight?: boolean;
  highlighted?: boolean;
  /** The ref is hidden from the graph. */
  hidden?: boolean;
  /** The graph is focused on the author of the commit the menu is about. */
  authorFocused?: boolean;
  /** The branch at the commit a pull request from the current branch would go into. */
  pullRequestTarget?: string | null;
  /** The checked-out branch, the source of a pull request made from a menu. */
  headBranch?: string | null;
  /** The branch the menu is about already has a pull request. */
  hasPullRequest?: boolean;
}

/**
 * The branch a pull request from the current branch goes into when it starts from a commit: the
 * first branch (local or remote) at that commit, other than the current one.
 */
export function pullRequestTargetOf(
  refs: readonly RefInfo[],
  headBranch: string | null,
): string | null {
  const names = refs
    .filter((ref) => ref.kind !== "tag")
    .map((ref) => (ref.kind === "remote" ? ref.name.slice(ref.name.indexOf("/") + 1) : ref.name))
    .filter((name) => name !== "HEAD" && name !== headBranch);
  return names[0] ?? null;
}

const READ_ONLY = "Read-only mode is on.";

const write = (ctx: MenuContext, item: MenuItem): MenuItem =>
  ctx.readOnly && !item.disabled ? { ...item, disabled: READ_ONLY } : item;

/** Actions on a commit of the graph. */
export function commitMenu(commit: { id: string; author: string }, ctx: MenuContext): MenuItem[] {
  const atHead = ctx.headId === commit.id;
  return [
    write(ctx, {
      id: "checkoutCommit",
      label: "Checkout this commit (detached HEAD)",
      disabled: atHead && ctx.detached ? "HEAD is already at this commit." : undefined,
    }),
    write(ctx, { id: "createBranch", label: "Create a branch here…" }),
    write(ctx, { id: "createTag", label: "Create a tag here…" }),
    ...(ctx.pullRequestTarget
      ? [
          {
            id: "pullRequestHere",
            label: `Create a pull request into ${ctx.pullRequestTarget}…`,
            disabled: ctx.detached ? "HEAD is detached: check out a branch first." : undefined,
          },
        ]
      : []),
    write(ctx, {
      id: "cherryPick",
      label: "Cherry-pick onto the current branch",
      separatorBefore: true,
      disabled: atHead ? "This commit is already the current one." : undefined,
    }),
    write(ctx, { id: "revert", label: "Revert this commit" }),
    write(ctx, { id: "reset", label: "Reset a branch to here…", danger: true }),
    write(ctx, {
      id: "rebase",
      label: "Rebase the current branch onto this commit…",
      separatorBefore: true,
      disabled: atHead ? "This commit is already the current one." : undefined,
    }),
    write(ctx, {
      id: "rebaseInteractive",
      label: "Rebase interactively from this commit…",
      disabled: atHead ? "This commit is already the current one." : undefined,
    }),
    {
      id: "focusAuthor",
      label: ctx.authorFocused ? "Remove the author focus" : `Focus on ${commit.author}`,
      separatorBefore: true,
    },
    { id: "copyId", label: "Copy the commit id" },
  ];
}

/** Actions on a branch, a remote branch or a tag. */
export function refMenu(ref: RefInfo, ctx: MenuContext): MenuItem[] {
  const view: MenuItem[] = [];
  if (ref.kind !== "tag") {
    if (ctx.canHighlight) {
      view.push({
        id: "highlight",
        label: ctx.highlighted ? "Remove the highlight" : "Highlight this branch",
        separatorBefore: true,
      });
    }
    view.push({
      id: "hide",
      label: ctx.hidden ? "Show in the graph" : "Hide from the graph",
      separatorBefore: !ctx.canHighlight,
    });
  }
  const branchName = ref.kind === "remote" ? ref.name.slice(ref.name.indexOf("/") + 1) : ref.name;
  const ticket = ref.kind === "tag" ? null : ticketFromBranch(branchName);
  const openTicket: MenuItem[] = ticket
    ? [{ id: "openTicket", label: `Open ticket ${ticket}` }]
    : [];
  // On the current branch there is one entry: open its pull request, or start one. On another
  // branch a pull request can be started from the current branch into it, and its own pull
  // request, when it has one, can be opened.
  const isCurrent =
    (ref.kind === "local" && ref.isHead) ||
    (ctx.headBranch != null && ctx.headBranch === branchName);
  const hasPullRequest = ctx.hasPullRequest === true;
  const pullRequestItems: MenuItem[] = isCurrent
    ? [
        {
          id: "pullRequest",
          label: hasPullRequest ? "Open the pull request" : "Create a pull request…",
          separatorBefore: true,
        },
      ]
    : [
        {
          id: "pullRequestInto",
          label: `Create a pull request into ${branchName}…`,
          separatorBefore: true,
          disabled: ctx.detached ? "HEAD is detached: check out a branch first." : undefined,
        },
        ...(hasPullRequest
          ? [{ id: "pullRequest", label: `Open the pull request of ${branchName}` }]
          : []),
      ];
  // Resets the checked-out branch to where this ref points.
  const resetHere = (disabled?: string): MenuItem =>
    write(ctx, {
      id: "reset",
      label: `Reset ${ctx.headBranch ?? "the current branch"} to ${ref.name}…`,
      danger: true,
      disabled:
        ctx.detached || !ctx.headBranch ? "HEAD is detached: check out a branch first." : disabled,
    });
  const copy: MenuItem = {
    id: "copyName",
    label: "Copy the name",
    separatorBefore: ref.kind === "tag",
  };

  if (ref.kind === "local") {
    return [
      write(ctx, {
        id: "checkout",
        label: `Checkout ${ref.name}`,
        disabled: ref.isHead ? "This branch is already checked out." : undefined,
      }),
      write(ctx, { id: "rename", label: "Rename…", separatorBefore: true }),
      write(ctx, { id: "upstream", label: "Set the upstream…" }),
      write(ctx, { id: "push", label: "Push…" }),
      write(ctx, {
        id: "merge",
        label: `Merge ${ref.name} into the current branch…`,
        disabled: ref.isHead ? "This is the current branch." : undefined,
      }),
      write(ctx, {
        id: "rebase",
        label: `Rebase the current branch onto ${ref.name}…`,
        disabled: ref.isHead ? "This is the current branch." : undefined,
      }),
      resetHere(ref.isHead ? "This is the current branch." : undefined),
      ...pullRequestItems,
      ...openTicket,
      write(ctx, {
        id: "delete",
        label: "Delete…",
        danger: true,
        disabled: ref.isHead ? "The checked-out branch cannot be deleted." : undefined,
      }),
      ...view,
      copy,
    ];
  }
  if (ref.kind === "remote") {
    return [
      write(ctx, { id: "checkout", label: "Checkout as a local branch" }),
      write(ctx, { id: "merge", label: `Merge ${ref.name} into the current branch…` }),
      write(ctx, { id: "rebase", label: `Rebase the current branch onto ${ref.name}…` }),
      resetHere(),
      ...pullRequestItems,
      ...openTicket,
      write(ctx, {
        id: "deleteRemote",
        label: "Delete on the remote…",
        danger: true,
        separatorBefore: true,
      }),
      ...view,
      copy,
    ];
  }
  return [
    write(ctx, { id: "checkout", label: "Checkout (detached HEAD)" }),
    write(ctx, { id: "pushTag", label: "Push the tag to a remote…" }),
    write(ctx, { id: "deleteTag", label: "Delete the tag…", danger: true, separatorBefore: true }),
    copy,
  ];
}

/** Actions on a lane line. */
export function laneMenu(ctx: MenuContext & { canHide: boolean }): MenuItem[] {
  const items: MenuItem[] = [
    {
      id: "highlight",
      label: ctx.highlighted ? "Remove the highlight" : "Highlight this branch",
    },
  ];
  if (ctx.canHide) items.push({ id: "hide", label: "Hide this branch" });
  return items;
}

/** Actions on a working tree of the sidebar. */
export function worktreeMenu(
  tree: Worktree,
  ctx: { readOnly: boolean; editorLabel: string; terminalLabel: string; fileManager?: string },
): MenuItem[] {
  const gone = tree.prunable !== null;
  const items: MenuItem[] = [
    { id: "open", label: "Open in a tab", disabled: gone ? "The folder is gone." : undefined },
    {
      id: "editor",
      label: `Open in ${ctx.editorLabel}`,
      disabled: gone ? "The folder is gone." : undefined,
    },
    {
      id: "terminal",
      label: `Open in ${ctx.terminalLabel}`,
      disabled: gone ? "The folder is gone." : undefined,
    },
    {
      id: "reveal",
      label: `Reveal in ${ctx.fileManager ?? "Finder"}`,
      disabled: gone ? "The folder is gone." : undefined,
    },
    { id: "copyPath", label: "Copy the path" },
  ];
  if (!tree.isMain) {
    items.push({
      id: "lock",
      label: tree.locked === null ? "Lock" : "Unlock",
      separatorBefore: true,
      disabled: ctx.readOnly ? READ_ONLY : undefined,
    });
  }
  if (gone) {
    items.push({
      id: "prune",
      label: "Prune the working trees whose folder is gone",
      disabled: ctx.readOnly ? READ_ONLY : undefined,
    });
  }
  if (!tree.isMain) {
    items.push({
      id: "remove",
      label: "Remove the working tree…",
      danger: true,
      disabled: ctx.readOnly ? READ_ONLY : undefined,
    });
  }
  return items;
}
