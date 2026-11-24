<script setup lang="ts">
import {
  Archive,
  ArrowDown,
  ArrowUp,
  Crosshair,
  Lock,
  LockOpen,
  RefreshCw,
  ScrollText,
  Search,
} from "@lucide/vue";
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import BranchDialogs from "@/components/BranchDialogs.vue";
import ContextMenu from "@/components/ContextMenu.vue";
import ActivityPanel from "@/components/ActivityPanel.vue";
import CommitPanel from "@/components/CommitPanel.vue";
import DiffView from "@/components/DiffView.vue";
import IdentityDialog from "@/components/IdentityDialog.vue";
import LanePopover from "@/components/LanePopover.vue";
import SettingsButton from "@/components/SettingsButton.vue";
import SettingsDialog from "@/components/SettingsDialog.vue";
import ConflictResolver from "@/components/ConflictResolver.vue";
import HistoryDialogs from "@/components/HistoryDialogs.vue";
import MergeDialogs from "@/components/MergeDialogs.vue";
import NoticeHost from "@/components/NoticeHost.vue";
import RebaseDialog from "@/components/RebaseDialog.vue";
import OperationBanner from "@/components/OperationBanner.vue";
import RemoteDialogs from "@/components/RemoteDialogs.vue";
import RemoteStatus from "@/components/RemoteStatus.vue";
import MetroGraph, { type GraphContextTarget } from "@/components/graph/MetroGraph.vue";
import RepoSidebar from "@/components/RepoSidebar.vue";
import SearchBar from "@/components/SearchBar.vue";
import TabBar from "@/components/TabBar.vue";
import { Button } from "@/components/ui/button";
import WelcomeScreen from "@/components/WelcomeScreen.vue";
import WorkingPanel from "@/components/WorkingPanel.vue";
import { useGraphSettings } from "@/composables/useGraphSettings";
import { useFileMenu } from "@/composables/useFileMenu";
import { useIntegrations } from "@/composables/useIntegrations";
import { type AppMenuState, buildMenuModel, parseMenuAction } from "@/lib/appMenu";
import { revealInFinder } from "@/lib/api";
import { hasNativeMenu, setNativeMenu } from "@/lib/nativeMenu";
import { useSettings } from "@/composables/useSettings";
import { useWorkspace } from "@/composables/useWorkspace";
import type { FileChange, GraphRow, Lane, RefInfo, StashEntry, StatusEntry } from "@/lib/api";
import { commitMenu, laneMenu, type MenuItem, refMenu } from "@/lib/menus";
import { refsOfBranch } from "@/lib/branchRefs";
import { headSide as headSideOf, pickCheckoutRef } from "@/lib/navigation";
import type { WipRow } from "@/lib/metro/draw";

const workspace = useWorkspace();
const { tabs, active, activeId, diffMode, recentRepos, openError, opening } = workspace;

const graph = ref<InstanceType<typeof MetroGraph>>();
const searchBar = ref<InstanceType<typeof SearchBar>>();
const lanePopover = ref<{ lane: Lane; x: number; y: number } | null>(null);

/** Name of the highlighted lane, for the chip in the toolbar. */
const highlightName = computed(() => {
  const tab = active.value;
  if (!tab || tab.highlightLane === null) return "";
  return tab.source?.meta.lanes[tab.highlightLane]?.name ?? "this line";
});

/** The search box, floating at the top right of the graph: opened by the magnifier or Ctrl/Cmd+F. */
const searchOpen = ref(false);

async function openSearch() {
  searchOpen.value = true;
  await nextTick();
  searchBar.value?.focus();
}

function closeSearch() {
  searchOpen.value = false;
  if (active.value) workspace.clearSearch(active.value);
}

function toggleSearch() {
  if (searchOpen.value) closeSearch();
  else void openSearch();
}

// A search belongs to one repository: the box closes when another one is shown.
watch(activeId, () => (searchOpen.value = false));

async function runSearch(query: string) {
  const tab = active.value;
  if (!tab) return;
  await workspace.search(tab, query);
  jumpToMatch(workspace.active.value?.search.rows[workspace.active.value.search.current]);
}

function stepMatch(delta: 1 | -1) {
  const tab = active.value;
  if (tab) jumpToMatch(workspace.stepMatch(tab, delta));
}

function jumpToMatch(row: number | null | undefined) {
  if (row !== null && row !== undefined) graph.value?.focusRow(row);
}

function highlightFromPopover() {
  const tab = active.value;
  if (tab && lanePopover.value) workspace.toggleHighlight(tab, lanePopover.value.lane.id);
  lanePopover.value = null;
}

function hideFromPopover() {
  const tab = active.value;
  const name = lanePopover.value?.lane.name;
  lanePopover.value = null;
  if (tab && name) void workspace.hideBranch(tab, name);
}

const canHideLane = computed(() => {
  const tab = active.value;
  const name = lanePopover.value?.lane.name;
  return !!tab && !!name && refsOfBranch(tab.refs, name).length > 0;
});

type MenuTarget =
  | { kind: "ref"; ref: RefInfo; commitId: string | null }
  | { kind: "commit"; id: string }
  | { kind: "lane"; lane: Lane };
const menu = ref<{ target: MenuTarget; items: MenuItem[]; x: number; y: number } | null>(null);

function menuContext(canHighlight: boolean, highlighted: boolean, hidden: boolean) {
  const tab = active.value;
  const head = tab?.source?.meta.refs.find((ref) => ref.isHead);
  return {
    readOnly: tab?.readOnly ?? false,
    headId: head?.target ?? null,
    detached: !!tab && tab.headBranch === null,
    canHighlight,
    highlighted,
    hidden,
  };
}

/** Lane of a ref in the graph, if it is drawn. */
function laneOfRef(ref: RefInfo): number | null {
  const found = active.value?.source?.meta.refs.find((r) => r.fullName === ref.fullName);
  return found ? found.lane : null;
}

function openRefMenu(ref: RefInfo, x: number, y: number, commitId: string | null) {
  const tab = active.value;
  if (!tab) return;
  const lane = laneOfRef(ref);
  menu.value = {
    target: { kind: "ref", ref, commitId },
    items: refMenu(
      ref,
      menuContext(
        lane !== null,
        lane !== null && tab.highlightLane === lane,
        tab.hidden.has(ref.fullName),
      ),
    ),
    x,
    y,
  };
}

/** The list of what a `+N` label hides; choosing one opens the menu of that branch. */
const moreMenu = ref<{
  refs: RefInfo[];
  commitId: string | null;
  items: MenuItem[];
  x: number;
  y: number;
} | null>(null);

function onMoreRefs(target: {
  items: Array<{ label: string; ref: RefInfo }>;
  commit: GraphRow | undefined;
  clientX: number;
  clientY: number;
}) {
  moreMenu.value = {
    refs: target.items.map((item) => item.ref),
    commitId: target.commit?.id ?? null,
    items: target.items.map((item, index) => ({ id: String(index), label: item.label })),
    x: target.clientX,
    y: target.clientY,
  };
}

function onMoreSelect(id: string) {
  const current = moreMenu.value;
  moreMenu.value = null;
  const ref = current?.refs[Number(id)];
  if (current && ref) openRefMenu(ref, current.x, current.y, current.commitId);
}

watch(activeId, () => (moreMenu.value = null));

function onGraphMenu(target: GraphContextTarget) {
  const tab = active.value;
  if (!tab) return;
  const { clientX: x, clientY: y } = target;
  if (target.kind === "ref") {
    openRefMenu(target.ref, x, y, target.commit?.id ?? null);
  } else if (target.kind === "commit") {
    menu.value = {
      target: { kind: "commit", id: target.commit.id },
      items: commitMenu(target.commit, menuContext(false, false, false)),
      x,
      y,
    };
  } else {
    const name = target.lane.name;
    const canHide = name !== null && refsOfBranch(tab.refs, name).length > 0;
    menu.value = {
      target: { kind: "lane", lane: target.lane },
      items: laneMenu({
        ...menuContext(true, tab.highlightLane === target.lane.id, false),
        canHide,
      }),
      x,
      y,
    };
  }
}

function onStash(action: "apply" | "pop" | "drop", stash: StashEntry) {
  const tab = active.value;
  if (!tab) return;
  if (action === "drop") workspace.askDropStash(tab, stash);
  else void workspace.applyStash(tab, stash, action === "pop");
}

function copy(text: string) {
  void navigator.clipboard?.writeText(text).catch(() => {});
}

function onMenuSelect(id: string) {
  const tab = active.value;
  const current = menu.value;
  menu.value = null;
  if (!tab || !current) return;
  const t = current.target;
  if (t.kind === "commit") {
    const short = t.id.slice(0, 7);
    if (id === "checkoutCommit") void workspace.checkout(tab, { kind: "commit", id: t.id }, null);
    else if (id === "createBranch") workspace.askCreateBranch(tab, { id: t.id, label: short });
    else if (id === "createTag") workspace.askCreateTag(tab, { id: t.id, label: short });
    else if (id === "cherryPick") void workspace.cherryPick(tab, { id: t.id, label: short });
    else if (id === "revert") void workspace.revert(tab, { id: t.id, label: short });
    else if (id === "reset") workspace.askReset(tab, { id: t.id, label: short });
    else if (id === "rebase") void workspace.askRebase(tab, t.id);
    else if (id === "rebaseInteractive") void workspace.askRebase(tab, t.id, true);
    else if (id === "copyId") copy(t.id);
  } else if (t.kind === "lane") {
    if (id === "highlight") workspace.toggleHighlight(tab, t.lane.id);
    else if (id === "hide" && t.lane.name) void workspace.hideBranch(tab, t.lane.name);
  } else {
    const { ref } = t;
    const target = { kind: ref.kind === "local" ? "branch" : ref.kind, name: ref.name } as const;
    if (id === "checkout") void workspace.checkout(tab, target, null);
    else if (id === "rename") workspace.askRenameBranch(tab, ref.name);
    else if (id === "upstream") workspace.askUpstream(tab, ref.name);
    else if (id === "delete") workspace.askDeleteBranch(tab, ref.name);
    else if (id === "push") void workspace.askPush(tab, ref.name);
    else if (id === "merge") void workspace.askMerge(tab, ref.name);
    else if (id === "rebase") void workspace.askRebase(tab, ref.name);
    else if (id === "deleteRemote") {
      const slash = ref.name.indexOf("/");
      workspace.askDeleteRemoteBranch(tab, ref.name.slice(0, slash), ref.name.slice(slash + 1));
    } else if (id === "deleteTag") workspace.askDeleteTag(tab, ref.name);
    else if (id === "highlight") {
      const lane = laneOfRef(ref);
      if (lane !== null) workspace.toggleHighlight(tab, lane);
    } else if (id === "hide") workspace.toggleRef(tab, ref.fullName);
    else if (id === "copyName") copy(ref.name);
    else if (id === "pullRequest") {
      const slash = ref.name.indexOf("/");
      void workspace.openPullRequest(
        tab,
        ref.kind === "remote" ? ref.name.slice(slash + 1) : ref.name,
      );
    }
  }
}

// A menu belongs to one repository.
watch(activeId, () => (menu.value = null));

// ---------- where HEAD is ----------

/** The ref HEAD points at (the checked-out branch, or the detached position). */
const headRef = computed(() => active.value?.source?.meta.refs.find((ref) => ref.isHead) ?? null);
const headRow = computed(() => headRef.value?.row ?? null);

/** The rows in view, to tell whether HEAD is above or below. */
const visibleRows = ref<[number, number] | null>(null);
watch(activeId, () => (visibleRows.value = null));
const headSide = computed(() => headSideOf(headRow.value, visibleRows.value));
const headLabel = computed(() => active.value?.headBranch ?? "detached HEAD");

function goToHead() {
  if (headRow.value !== null) graph.value?.focusRow(headRow.value);
}

// The graph goes to HEAD when a repository is opened and whenever HEAD moves (a checkout, a
// commit, a pull, a rebase); a refresh that only adds commits elsewhere leaves the view alone.
const centeredOn = new Map<string, string>();
watch(
  () => [activeId.value, headRef.value?.target ?? null, headRow.value] as const,
  async ([id, target, row]) => {
    if (id === null || target === null || row === null) return;
    if (centeredOn.get(id) === target) return;
    centeredOn.set(id, target);
    await nextTick();
    graph.value?.focusRow(row);
  },
  { flush: "post" },
);

/** Double click: the last commit of a branch (or its label) becomes the checked-out branch. */
function onActivate(target: { commit: GraphRow; ref: RefInfo | null }) {
  const tab = active.value;
  if (!tab) return;
  const choice = pickCheckoutRef(target.ref ? [target.ref] : target.commit.refs);
  if (!choice) return;
  if (choice.kind === "local" && choice.isHead) {
    workspace.notify("info", `You are already on ${choice.name}.`);
    return;
  }
  void workspace.checkout(tab, {
    kind: choice.kind === "local" ? "branch" : "remote",
    name: choice.name,
  });
}

/** The working tree row pinned above the graph. */
const wipRow = computed<WipRow | null>(() => {
  const tab = active.value;
  if (!tab?.source || !tab.status) return null;
  const meta = tab.source.meta;
  const head = meta.refs.find((ref) => ref.isHead);
  const col = head ? (meta.lanes[head.lane]?.col ?? 0) : 0;
  const paths = new Set([
    ...tab.status.staged.map((e) => e.path),
    ...tab.status.unstaged.map((e) => e.path),
    ...tab.status.conflicted.map((e) => e.path),
  ]);
  return { col, count: paths.size, selected: tab.wip };
});

/** The right panel shows the working tree when its row is selected, and also when there is no
 * history to select (a repository without commits). */
const showWorking = computed(() => {
  const tab = active.value;
  return !!tab && (tab.wip || (tab.source?.meta.total === 0 && tab.status !== null));
});

const WORK_STATUS = {
  added: "added",
  untracked: "added",
  modified: "modified",
  deleted: "deleted",
  renamed: "renamed",
  copied: "copied",
  typeChanged: "typeChanged",
} as const;

/** A changed file of the working tree in the shape the diff view takes. */
const workFile = (entry: StatusEntry): FileChange => ({
  path: entry.path,
  oldPath: entry.oldPath,
  status: WORK_STATUS[entry.state],
  additions: null,
  deletions: null,
  binary: false,
});

function toggleActivity() {
  workspace.activityOpen.value = !workspace.activityOpen.value;
}

// The activity log is read when it is shown, and when another repository is shown.
watch(
  () => [workspace.activityOpen.value, activeId.value],
  () => {
    if (workspace.activityOpen.value && active.value) void workspace.loadActivity(active.value);
  },
);

const settings = useSettings();
const graphSettings = useGraphSettings();
const density = graphSettings.density;

// ---------- the menu bar (macOS) ----------

const integrations = useIntegrations();
const { fileMenu, closeFileMenu } = useFileMenu();

const fileMenuItems = computed<MenuItem[]>(() => [
  { id: "openEditor", label: `Open in ${integrations.editorLabel()}` },
  { id: "openTerminal", label: `Open ${integrations.terminalLabel()} in the repository` },
  { id: "copyPath", label: "Copy the path", separatorBefore: true },
]);

function onFileMenuSelect(id: string) {
  const tab = active.value;
  const target = fileMenu.value;
  closeFileMenu();
  if (!tab || !target) return;
  if (id === "openEditor") void workspace.openInEditor(tab, integrations.editor.value, target.path);
  else if (id === "openTerminal") void workspace.openInTerminal(tab, integrations.terminal.value);
  else if (id === "copyPath") copy(target.path);
}

// A menu belongs to one repository.
watch(activeId, closeFileMenu);

const menuState = computed<AppMenuState>(() => {
  const tab = active.value;
  return {
    hasRepo: !!tab,
    canWrite: !!tab && !tab.readOnly && !tab.busy,
    remoteBusy: !!tab && tab.remote !== null,
    readOnly: tab?.readOnly ?? false,
    detached: !!tab && tab.headBranch === null,
    hasHead: headRow.value !== null,
    tabCount: tabs.value.length,
    recents: recentRepos.value.map((r) => ({ path: r.path, name: r.name })),
    branches: (tab?.refs ?? [])
      .filter((ref) => ref.kind === "local")
      .map((ref) => ({ name: ref.name, isHead: ref.isHead })),
    density: density.value,
    activityOpen: workspace.activityOpen.value,
    editorLabel: integrations.editorLabel(),
    terminalLabel: integrations.terminalLabel(),
  };
});

function shiftTab(delta: 1 | -1) {
  const list = tabs.value;
  if (list.length < 2) return;
  const index = list.findIndex((tab) => tab.id === activeId.value);
  workspace.activate(list[(index + delta + list.length) % list.length].id);
}

/** What a choice in the menu bar does: the same things the toolbar and the sidebar do. */
function onMenuAction(id: string) {
  const { name, arg } = parseMenuAction(id);
  const tab = active.value;
  switch (name) {
    case "settings":
      settings.openSettings();
      return;
    case "open":
      void workspace.pickAndOpen();
      return;
    case "recent":
      void workspace.openPath(arg);
      return;
    case "clearRecents":
      workspace.clearRecents();
      return;
    case "nextTab":
      shiftTab(1);
      return;
    case "previousTab":
      shiftTab(-1);
      return;
  }
  if (!tab) return;
  switch (name) {
    case "closeTab":
      void workspace.closeTab(tab.id);
      break;
    case "fetch":
      void workspace.fetchRemote(tab);
      break;
    case "pull":
      void workspace.pull(tab);
      break;
    case "push":
      void workspace.push(tab);
      break;
    case "refresh":
      void workspace.refreshGraph(tab);
      break;
    case "readOnly":
      void workspace.setReadOnly(tab, !tab.readOnly);
      break;
    case "openEditor":
      void workspace.openInEditor(tab, integrations.editor.value, null);
      break;
    case "openTerminal":
      void workspace.openInTerminal(tab, integrations.terminal.value);
      break;
    case "pullRequest":
      if (tab.headBranch) void workspace.openPullRequest(tab, tab.headBranch);
      break;
    case "reveal":
      void revealInFinder(tab.path).catch(() =>
        workspace.notify("error", "Could not open Finder."),
      );
      break;
    case "copyPath":
      copy(tab.path);
      break;
    case "newBranch":
      workspace.askCreateBranch(tab, null);
      break;
    case "checkout":
      void workspace.checkout(tab, { kind: "branch", name: arg });
      break;
    case "merge":
      void workspace.askMerge(tab, arg);
      break;
    case "rebase":
      void workspace.askRebase(tab, arg);
      break;
    case "stash":
      workspace.askStash(tab);
      break;
    case "search":
      void openSearch();
      break;
    case "goToHead":
      goToHead();
      break;
    case "density":
      graphSettings.setDensity(arg === "detailed" ? "detailed" : "compact");
      break;
    case "activity":
      toggleActivity();
      break;
  }
}

if (hasNativeMenu()) {
  watch(
    menuState,
    (state) => void setNativeMenu(buildMenuModel(state), onMenuAction).catch(() => {}),
    { immediate: true, deep: true },
  );
}

// Ctrl/Cmd+F goes to the search box.
function onKeydown(event: KeyboardEvent) {
  // Cmd+, (Ctrl+, elsewhere) opens the settings, as in most desktop apps.
  if ((event.ctrlKey || event.metaKey) && event.key === ",") {
    event.preventDefault();
    settings.openSettings();
    return;
  }
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f" && active.value) {
    event.preventDefault();
    openSearch();
  }
}
onMounted(() => document.addEventListener("keydown", onKeydown));
onBeforeUnmount(() => document.removeEventListener("keydown", onKeydown));
</script>

<template>
  <div class="bg-background text-foreground flex h-screen flex-col">
    <TabBar
      :tabs="tabs"
      :active-id="activeId"
      @activate="workspace.activate"
      @close="workspace.closeTab"
      @open="workspace.pickAndOpen"
    />

    <div v-if="active" class="relative flex min-h-0 flex-1">
      <div class="flex w-64 shrink-0 flex-col border-r">
        <div class="min-h-0 flex-1">
          <RepoSidebar
            :tab="active"
            @toggle-ref="workspace.toggleRef(active, $event)"
            @toggle-refs="workspace.toggleRefs(active, $event)"
            @toggle-kind="workspace.toggleKind(active, $event)"
            @toggle-highlight="workspace.toggleHighlight(active, $event)"
            @jump="graph?.focusRow($event)"
            @ref-menu="(ref, x, y) => openRefMenu(ref, x, y, null)"
            @stash="onStash"
          />
        </div>
        <div class="border-t p-1.5">
          <SettingsButton />
        </div>
      </div>

      <main class="flex min-w-0 flex-1 flex-col">
        <div class="flex flex-wrap items-center gap-x-2 gap-y-1.5 border-b px-3 py-1.5 text-xs">
          <span class="text-muted-foreground min-w-0 truncate" :title="active.path">
            {{ active.path }}
            <template v-if="active.headBranch"> · {{ active.headBranch }}</template>
          </span>
          <span v-if="active.loading" class="text-muted-foreground">Updating the graph…</span>
          <button
            v-if="active.highlightLane !== null"
            class="bg-accent hover:bg-accent/70 flex shrink-0 items-center gap-1 rounded px-2 py-0.5 whitespace-nowrap"
            title="Remove the highlight"
            @click="workspace.clearHighlight(active)"
          >
            Highlighting {{ highlightName }} ✕
          </button>
          <div class="ml-auto flex min-w-0 flex-wrap items-center justify-end gap-2">
            <Button
              size="sm"
              :variant="searchOpen ? 'default' : 'outline'"
              title="Search the commits (Cmd/Ctrl+F)"
              aria-label="Search the commits"
              :aria-pressed="searchOpen"
              @click="toggleSearch"
            >
              <Search class="size-3.5" />
              Search
            </Button>
            <Button
              size="sm"
              variant="outline"
              :disabled="headRow === null"
              :title="`Scroll the graph to where you are (${headLabel})`"
              @click="goToHead"
            >
              <Crosshair class="size-3.5" />
              Go to HEAD
            </Button>
            <div class="flex gap-1" role="group" aria-label="Remotes">
              <Button
                size="sm"
                variant="outline"
                :title="active.readOnly ? 'Read-only mode is on.' : 'Put the changes aside'"
                :disabled="active.busy || active.readOnly"
                @click="workspace.askStash(active)"
              >
                <Archive class="size-3.5" />
                Stash
              </Button>
              <Button
                size="sm"
                variant="outline"
                title="Download new commits from the remotes"
                :disabled="active.remote !== null"
                @click="workspace.fetchRemote(active)"
              >
                <RefreshCw class="size-3.5" />
                Fetch
              </Button>
              <Button
                size="sm"
                variant="outline"
                :title="
                  active.readOnly
                    ? 'Read-only mode is on.'
                    : 'Fetch and bring the upstream commits into this branch'
                "
                :disabled="active.remote !== null || active.busy || active.readOnly"
                @click="workspace.pull(active)"
              >
                <ArrowDown class="size-3.5" />
                Pull
                <span
                  v-if="active.status?.behind"
                  class="bg-primary text-primary-foreground rounded px-1 text-[10px] leading-4"
                  :title="`${active.status.behind} commit(s) to bring in`"
                >
                  {{ active.status.behind }}
                </span>
              </Button>
              <Button
                size="sm"
                variant="outline"
                :title="
                  active.readOnly ? 'Read-only mode is on.' : 'Send this branch to its upstream'
                "
                :disabled="active.remote !== null || active.busy || active.readOnly"
                @click="workspace.push(active)"
              >
                <ArrowUp class="size-3.5" />
                Push
                <span
                  v-if="active.status?.ahead"
                  class="bg-primary text-primary-foreground rounded px-1 text-[10px] leading-4"
                  :title="`${active.status.ahead} commit(s) to send`"
                >
                  {{ active.status.ahead }}
                </span>
              </Button>
            </div>
            <Button
              size="sm"
              :variant="active.readOnly ? 'default' : 'outline'"
              :title="
                active.readOnly
                  ? 'Read-only mode is on: nothing can be changed. Click to allow changes.'
                  : 'Allow changes (click for read-only mode)'
              "
              :aria-pressed="active.readOnly"
              @click="workspace.setReadOnly(active, !active.readOnly)"
            >
              <component :is="active.readOnly ? Lock : LockOpen" class="size-3.5" />
              {{ active.readOnly ? "Read-only" : "Editable" }}
            </Button>
            <Button
              size="sm"
              :variant="workspace.activityOpen.value ? 'default' : 'outline'"
              title="The git commands run by the app"
              @click="toggleActivity"
            >
              <ScrollText class="size-3.5" />
              Activity
            </Button>
          </div>
        </div>

        <div
          v-if="active.error"
          class="flex items-center gap-2 border-b px-3 py-2 text-sm"
          role="alert"
        >
          <span class="text-destructive min-w-0 flex-1 break-words">{{ active.error }}</span>
          <Button size="sm" variant="outline" @click="workspace.retry(active)">Retry</Button>
          <Button size="sm" variant="ghost" @click="active.error = ''">Dismiss</Button>
        </div>

        <OperationBanner :tab="active" />

        <div class="relative min-h-0 flex-1">
          <SearchBar
            v-if="searchOpen"
            :key="active.id"
            ref="searchBar"
            class="absolute top-2 right-6 z-30 shadow-md"
            :query="active.search.query"
            :total="active.search.total"
            :current="active.search.current"
            :loading="active.search.loading"
            :error="active.search.error"
            @search="runSearch"
            @step="stepMatch"
            @clear="workspace.clearSearch(active)"
            @close="closeSearch"
          />
          <MetroGraph
            v-if="active.source && active.source.meta.total > 0"
            :key="active.id"
            ref="graph"
            :density="density"
            :selected="active.selected"
            :initial-scroll-top="active.scrollTop"
            :anchor="active.anchor"
            :meta="active.source.meta"
            :fetch-page="active.source.fetchPage"
            :highlight-lane="active.highlightLane"
            :matches="active.search.rows"
            :wip="wipRow"
            :head-row="headRow"
            :initials="graphSettings.showInitials.value"
            @update:density="graphSettings.setDensity"
            @wip-click="workspace.selectWip(active)"
            @lane-click="lanePopover = $event"
            @context-menu="onGraphMenu"
            @more-refs="onMoreRefs"
            @activate="onActivate"
            @range="(first, last) => (visibleRows = [first, last])"
            @update:selected="workspace.select(active, $event)"
            @update:scroll-top="active.scrollTop = $event"
            @update:top-commit="active.topAnchor = $event"
            @select="workspace.select(active, $event.row, $event)"
            @error="active.error = $event"
          />
          <button
            v-if="headSide"
            class="bg-primary text-primary-foreground absolute left-1/2 z-10 -translate-x-1/2 rounded-full px-3 py-1 text-xs shadow-md"
            :class="headSide === 'above' ? 'top-12' : 'bottom-3'"
            :title="`You are on ${headLabel}: click to go there`"
            @click="goToHead"
          >
            {{ headSide === "above" ? "↑" : "↓" }} HEAD · {{ headLabel }}
          </button>
          <LanePopover
            v-if="lanePopover && active.source"
            :lane="lanePopover.lane"
            :x="lanePopover.x"
            :y="lanePopover.y"
            :highlighted="active.highlightLane === lanePopover.lane.id"
            :can-hide="canHideLane"
            @highlight="highlightFromPopover"
            @hide="hideFromPopover"
            @close="lanePopover = null"
          />
          <div
            v-else-if="!active.source || active.source.meta.total === 0"
            class="text-muted-foreground absolute inset-0 flex flex-col items-center justify-center gap-3 p-6 text-center text-sm"
          >
            <template v-if="active.loading">Reading the history…</template>
            <template v-else-if="active.source && active.hidden.size > 0">
              <p>Every branch and tag is hidden, so there is nothing to draw.</p>
              <Button size="sm" @click="workspace.showAllRefs(active)">Show all refs</Button>
            </template>
            <template v-else-if="active.source">
              <p>This repository has no commits yet.</p>
            </template>
            <template v-else>
              <p>The graph is not available.</p>
              <Button size="sm" variant="outline" @click="workspace.retry(active)"
                >Try again</Button
              >
            </template>
          </div>
          <DiffView
            v-if="active.workDiff"
            :key="`work:${active.workDiff.area}:${active.workDiff.entry.path}`"
            v-model:mode="diffMode"
            class="absolute inset-0 z-20"
            :file="workFile(active.workDiff.entry)"
            :commit-id="`work:${active.workDiff.area}`"
            :version="active.workDiffVersion"
            :heading="active.workDiff.area === 'staged' ? 'Staged changes' : 'Unstaged changes'"
            :load="(force) => workspace.workingDiff(active!, force)"
            :hunk-action="{
              label: active.workDiff.area === 'unstaged' ? 'Stage hunk' : 'Unstage hunk',
              disabled: active.busy || active.readOnly,
              run: (id) =>
                active!.workDiff?.area === 'unstaged'
                  ? workspace.stageHunk(active!, active!.workDiff.entry.path, id)
                  : workspace.unstageHunk(active!, active!.workDiff!.entry.path, id),
            }"
            @close="workspace.closeWorkDiff(active)"
          />
          <DiffView
            v-else-if="active.openFile && active.selectedCommit"
            :key="`${active.selectedCommit.id}:${active.openFile.path}`"
            v-model:mode="diffMode"
            class="absolute inset-0 z-20"
            :file="active.openFile"
            :commit-id="active.selectedCommit.id"
            :load="(force) => workspace.fileDiff(active!, force)"
            @close="workspace.openFile(active, null)"
          />
          <ConflictResolver
            v-if="active.resolverPath !== null"
            :key="`resolve:${active.id}`"
            :tab="active"
            class="absolute inset-0 z-30"
          />
        </div>
      </main>

      <div class="w-80 shrink-0 border-l">
        <WorkingPanel v-if="showWorking" :tab="active" />
        <CommitPanel
          v-else
          :commit="active.selectedCommit"
          :detail="active.detail"
          :loading="active.detailLoading"
          :error="active.detailError"
          :open-path="active.openFile?.path ?? null"
          @open-file="workspace.openFile(active, $event)"
        />
      </div>

      <ActivityPanel
        v-if="workspace.activityOpen.value"
        :tab="active"
        @close="workspace.activityOpen.value = false"
      />
      <IdentityDialog :tab="active" />
      <BranchDialogs :tab="active" />
      <RemoteDialogs :tab="active" />
      <HistoryDialogs :tab="active" />
      <MergeDialogs :tab="active" />
      <RebaseDialog :tab="active" />
      <RemoteStatus :tab="active" />
      <ContextMenu
        v-if="moreMenu"
        :items="moreMenu.items"
        :x="moreMenu.x"
        :y="moreMenu.y"
        @select="onMoreSelect"
        @close="moreMenu = null"
      />
      <ContextMenu
        v-if="fileMenu"
        :items="fileMenuItems"
        :x="fileMenu.x"
        :y="fileMenu.y"
        @select="onFileMenuSelect"
        @close="closeFileMenu"
      />
      <ContextMenu
        v-if="menu"
        :items="menu.items"
        :x="menu.x"
        :y="menu.y"
        @select="onMenuSelect"
        @close="menu = null"
      />
    </div>

    <WelcomeScreen
      v-else
      class="min-h-0 flex-1"
      :recents="recentRepos"
      :error="openError"
      :busy="opening"
      @open="workspace.pickAndOpen"
      @open-recent="workspace.openPath"
      @forget="workspace.forgetRecent"
    />

    <p v-if="active && openError" class="text-destructive border-t px-3 py-1 text-xs" role="alert">
      {{ openError }}
    </p>
    <div v-if="!active" class="border-t p-1.5">
      <SettingsButton />
    </div>
    <SettingsDialog />
    <NoticeHost />
  </div>
</template>
