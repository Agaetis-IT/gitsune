/** What the application menu needs to know to label and enable its entries. */
export interface AppMenuState {
  /** A repository is open (a tab is shown). */
  hasRepo: boolean;
  /** Writes are allowed: not read-only and nothing is writing. */
  canWrite: boolean;
  /** The repository has a remote operation running (fetch, pull, push). */
  remoteBusy: boolean;
  readOnly: boolean;
  /** The checked-out branch is detached or there is no branch. */
  detached: boolean;
  /** HEAD is in the graph (so "Go to HEAD" has a place to go). */
  hasHead: boolean;
  tabCount: number;
  recents: ReadonlyArray<{ path: string; name: string }>;
  /** Local branches of the shown repository. */
  branches: ReadonlyArray<{ name: string; isHead: boolean }>;
  density: "compact" | "detailed";
  activityOpen: boolean;
  historyOpen: boolean;
  /** The names of the editor and the terminal chosen in the settings. */
  editorLabel: string;
  terminalLabel: string;
  /** What the program that shows folders is called (Finder, Explorer...). */
  fileManager: string;
}

export type PredefinedItem =
  | "About"
  | "Services"
  | "Hide"
  | "HideOthers"
  | "ShowAll"
  | "Quit"
  | "Undo"
  | "Redo"
  | "Cut"
  | "Copy"
  | "Paste"
  | "SelectAll"
  | "Fullscreen"
  | "Minimize"
  | "Maximize";

export type MenuNode =
  | {
      kind: "item";
      /** What the application does when it is chosen (see `parseMenuAction`). */
      id: string;
      label: string;
      accelerator?: string;
      enabled: boolean;
      /** Set for an entry that shows a state (a check mark). */
      checked?: boolean;
    }
  | { kind: "submenu"; label: string; items: MenuNode[]; enabled?: boolean }
  | { kind: "predefined"; item: PredefinedItem }
  | { kind: "separator" };

/** The branches listed in a menu: more would make it unusable. */
export const MAX_MENU_BRANCHES = 30;
const MAX_MENU_RECENTS = 10;

const item = (
  id: string,
  label: string,
  enabled: boolean,
  accelerator?: string,
  checked?: boolean,
): MenuNode => ({ kind: "item", id, label, enabled, accelerator, checked });
const separator: MenuNode = { kind: "separator" };
const predefined = (name: PredefinedItem): MenuNode => ({ kind: "predefined", item: name });

/** A submenu of branches; `skipHead` leaves out the checked-out one (merge, rebase). */
function branchMenu(
  label: string,
  prefix: string,
  state: AppMenuState,
  enabled: boolean,
  skipHead: boolean,
): MenuNode {
  const names = state.branches
    .filter((branch) => !(skipHead && branch.isHead))
    .map((branch) => ({ ...branch }))
    .sort((a, b) => a.name.localeCompare(b.name));
  const shown = names.slice(0, MAX_MENU_BRANCHES);
  const items: MenuNode[] = shown.map((branch) =>
    item(
      `${prefix}:${branch.name}`,
      branch.name,
      enabled,
      undefined,
      skipHead ? undefined : branch.isHead,
    ),
  );
  if (names.length > shown.length) {
    items.push(item("none", `${names.length - shown.length} more: use the sidebar`, false));
  }
  if (items.length === 0) items.push(item("none", "No other branch", false));
  return { kind: "submenu", label, items, enabled: enabled && names.length > 0 };
}

/** The whole menu bar: the application, File, Edit, Repository, Branch, View, Window. */
export function buildMenuModel(state: AppMenuState): MenuNode[] {
  const repo = state.hasRepo;
  const write = repo && state.canWrite;
  const remote = repo && !state.remoteBusy;

  const recents: MenuNode[] = state.recents
    .slice(0, MAX_MENU_RECENTS)
    .map((entry) => item(`recent:${entry.path}`, entry.name, true));
  if (recents.length === 0) recents.push(item("none", "No recent repository", false));
  else recents.push(separator, item("clearRecents", "Clear the menu", true));

  return [
    {
      kind: "submenu",
      label: "git-hydra",
      items: [
        predefined("About"),
        separator,
        item("settings", "Settings…", true, "CmdOrCtrl+,"),
        item("diagnostics", "Diagnostics…", true),
        separator,
        predefined("Services"),
        separator,
        predefined("Hide"),
        predefined("HideOthers"),
        predefined("ShowAll"),
        separator,
        predefined("Quit"),
      ],
    },
    {
      kind: "submenu",
      label: "File",
      items: [
        item("open", "Open a Repository…", true, "CmdOrCtrl+O"),
        { kind: "submenu", label: "Open Recent", items: recents },
        separator,
        item("closeTab", "Close the Tab", repo, "CmdOrCtrl+W"),
        item("nextTab", "Next Tab", state.tabCount > 1, "Ctrl+Tab"),
        item("previousTab", "Previous Tab", state.tabCount > 1, "Ctrl+Shift+Tab"),
      ],
    },
    {
      kind: "submenu",
      label: "Edit",
      items: [
        predefined("Undo"),
        predefined("Redo"),
        separator,
        predefined("Cut"),
        predefined("Copy"),
        predefined("Paste"),
        predefined("SelectAll"),
      ],
    },
    {
      kind: "submenu",
      label: "Repository",
      items: [
        item("fetch", "Fetch", remote, "CmdOrCtrl+Shift+F"),
        item("pull", "Pull", remote && state.canWrite, "CmdOrCtrl+Shift+L"),
        item("push", "Push", remote && state.canWrite, "CmdOrCtrl+Shift+P"),
        separator,
        item("undoLast", "Undo Last Operation", write, "CmdOrCtrl+Alt+Z"),
        item("refresh", "Refresh", repo, "CmdOrCtrl+R"),
        item("readOnly", "Read-only Mode", repo, undefined, state.readOnly),
        separator,
        item("openEditor", `Open in ${state.editorLabel}`, repo, "CmdOrCtrl+Shift+E"),
        item("openTerminal", `Open in ${state.terminalLabel}`, repo, "CmdOrCtrl+Shift+T"),
        item("reveal", `Reveal in ${state.fileManager}`, repo),
        item("copyPath", "Copy the Path", repo),
      ],
    },
    {
      kind: "submenu",
      label: "Branch",
      items: [
        item("newBranch", "New Branch…", write, "CmdOrCtrl+Shift+N"),
        branchMenu("Check Out", "checkout", state, write, false),
        branchMenu("Merge into the Current Branch", "merge", state, write && !state.detached, true),
        branchMenu("Rebase onto", "rebase", state, write && !state.detached, true),
        item("pullRequest", "Open or Create a Pull Request", repo && !state.detached),
        separator,
        item("stash", "Stash…", write, "CmdOrCtrl+Shift+S"),
      ],
    },
    {
      kind: "submenu",
      label: "View",
      items: [
        item("palette", "Command Palette…", true, "CmdOrCtrl+K"),
        item("search", "Search the Commits", repo, "CmdOrCtrl+F"),
        item("goToHead", "Go to HEAD", repo && state.hasHead, "CmdOrCtrl+Shift+H"),
        separator,
        item("density:compact", "Compact Rows", repo, undefined, state.density === "compact"),
        item("density:detailed", "Detailed Rows", repo, undefined, state.density === "detailed"),
        item("toggleRemotes", "Hide or Show Remote Branches", repo),
        item("history", "History of Positions", repo, "CmdOrCtrl+Shift+Y", state.historyOpen),
        item("activity", "Activity Log", repo, "CmdOrCtrl+Shift+A", state.activityOpen),
        separator,
        predefined("Fullscreen"),
      ],
    },
    {
      kind: "submenu",
      label: "Window",
      items: [predefined("Minimize"), predefined("Maximize")],
    },
  ];
}

/** A menu action split into its name and its argument (`checkout:main` is `checkout`, `main`). */
export function parseMenuAction(id: string): { name: string; arg: string } {
  const colon = id.indexOf(":");
  return colon < 0 ? { name: id, arg: "" } : { name: id.slice(0, colon), arg: id.slice(colon + 1) };
}

/** The accelerators of the menu, to check that none is used twice. */
export function accelerators(nodes: readonly MenuNode[]): string[] {
  return nodes.flatMap((node) =>
    node.kind === "submenu"
      ? accelerators(node.items)
      : node.kind === "item" && node.accelerator
        ? [node.accelerator]
        : [],
  );
}
