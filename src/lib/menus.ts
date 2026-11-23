import type { RefInfo } from "@/lib/api";

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
}

const READ_ONLY = "Read-only mode is on.";

const write = (ctx: MenuContext, item: MenuItem): MenuItem =>
  ctx.readOnly && !item.disabled ? { ...item, disabled: READ_ONLY } : item;

/** Actions on a commit of the graph. */
export function commitMenu(commit: { id: string }, ctx: MenuContext): MenuItem[] {
  const atHead = ctx.headId === commit.id;
  return [
    write(ctx, {
      id: "checkoutCommit",
      label: "Checkout this commit (detached HEAD)",
      disabled: atHead && ctx.detached ? "HEAD is already at this commit." : undefined,
    }),
    write(ctx, { id: "createBranch", label: "Create a branch here…" }),
    write(ctx, { id: "createTag", label: "Create a tag here…" }),
    write(ctx, {
      id: "cherryPick",
      label: "Cherry-pick onto the current branch",
      separatorBefore: true,
      disabled: atHead ? "This commit is already the current one." : undefined,
    }),
    write(ctx, { id: "revert", label: "Revert this commit" }),
    write(ctx, { id: "reset", label: "Reset the current branch to here…", danger: true }),
    { id: "copyId", label: "Copy the commit id", separatorBefore: true },
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
