<script setup lang="ts">
import {
  Archive,
  ChevronDown,
  ChevronRight,
  Cloud,
  Eye,
  EyeOff,
  Focus,
  Folder,
  FolderOpen,
  GitBranch,
  Tag,
} from "@lucide/vue";
import { computed, ref } from "vue";
import type { RepoTab } from "@/composables/useWorkspace";
import type { RefInfo, RefKind, StashEntry } from "@/lib/api";
import { buildRows, folderPaths } from "@/lib/refTree";

const props = defineProps<{ tab: RepoTab }>();
const emit = defineEmits<{
  toggleRef: [fullName: string];
  /** Hide the refs of a folder, or show them again when all of them are hidden. */
  toggleRefs: [fullNames: string[]];
  toggleKind: [kind: RefKind];
  toggleHighlight: [lane: number];
  /** Right click on a ref (window coordinates). */
  refMenu: [ref: RefInfo, x: number, y: number];
  jump: [row: number];
  stash: [action: "apply" | "pop" | "drop", stash: StashEntry];
}>();

const stashesCollapsed = ref(false);

const GROUPS: Array<{ kind: RefKind; title: string; icon: typeof GitBranch }> = [
  { kind: "local", title: "Local branches", icon: GitBranch },
  { kind: "remote", title: "Remotes", icon: Cloud },
  { kind: "tag", title: "Tags", icon: Tag },
];

const collapsed = ref<Record<RefKind, boolean>>({ local: false, remote: false, tag: true });

/** Lane of each visible ref, to highlight its branch. */
const laneOf = computed(
  () => new Map((props.tab.source?.meta.refs ?? []).map((ref) => [ref.fullName, ref.lane])),
);

/** Row of each visible ref, to jump to its commit. */
const rowOf = computed(
  () => new Map((props.tab.source?.meta.refs ?? []).map((ref) => [ref.fullName, ref.row])),
);

// ---------- folders ----------

const FOLDERS_KEY = "git-hydra:sidebar-folders";

function readClosed(): Set<string> {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(FOLDERS_KEY) ?? "[]");
    return new Set(Array.isArray(parsed) ? parsed.filter((k) => typeof k === "string") : []);
  } catch {
    return new Set();
  }
}

/** The folders the user closed, per repository (`path|kind|folder`), kept between sessions. */
const closed = ref<Set<string>>(readClosed());

const folderKey = (kind: RefKind, path: string) => `${props.tab.path}|${kind}|${path}`;

function toggleFolder(kind: RefKind, path: string) {
  const next = new Set(closed.value);
  const key = folderKey(kind, path);
  if (!next.delete(key)) next.add(key);
  closed.value = next;
  try {
    localStorage.setItem(FOLDERS_KEY, JSON.stringify([...next]));
  } catch {
    // Not kept: the folders open again next time.
  }
}

function setAllFolders(kind: RefKind, refs: RefInfo[], close: boolean) {
  const next = new Set(closed.value);
  for (const path of folderPaths(refs)) {
    if (close) next.add(folderKey(kind, path));
    else next.delete(folderKey(kind, path));
  }
  closed.value = next;
  try {
    localStorage.setItem(FOLDERS_KEY, JSON.stringify([...next]));
  } catch {
    // Not kept.
  }
}

const groups = computed(() =>
  GROUPS.map((group) => {
    const refs = props.tab.refs.filter((ref) => ref.kind === group.kind);
    const hidden = refs.filter((ref) => props.tab.hidden.has(ref.fullName)).length;
    const closedHere = new Set(
      folderPaths(refs).filter((path) => closed.value.has(folderKey(group.kind, path))),
    );
    return { ...group, refs, hidden, rows: buildRows(refs, closedHere), folders: closedHere.size };
  }),
);

const indent = (depth: number) => ({ paddingLeft: `${20 + depth * 20}px` });

/** How many of these refs are hidden from the graph. */
const hiddenIn = (refs: RefInfo[]) => refs.filter((r) => props.tab.hidden.has(r.fullName)).length;

function onRefClick(ref: RefInfo) {
  const row = rowOf.value.get(ref.fullName);
  if (row !== undefined) emit("jump", row);
}
</script>

<template>
  <nav class="flex h-full flex-col overflow-y-auto text-sm" aria-label="Branches and tags">
    <section v-for="group in groups" :key="group.kind" class="border-b py-1">
      <div class="flex items-center pr-1">
        <button
          class="hover:bg-accent flex flex-1 items-center gap-1.5 rounded px-2 py-1.5 text-left font-medium"
          :aria-expanded="!collapsed[group.kind]"
          @click="collapsed[group.kind] = !collapsed[group.kind]"
        >
          <component :is="collapsed[group.kind] ? ChevronRight : ChevronDown" class="size-3.5" />
          <component :is="group.icon" class="text-muted-foreground size-3.5" />
          {{ group.title }}
          <span class="text-muted-foreground ml-auto text-xs font-normal">
            {{ group.refs.length - group.hidden }}/{{ group.refs.length }}
          </span>
        </button>
        <button
          v-if="group.rows.some((r) => r.kind === 'folder')"
          class="text-muted-foreground hover:bg-accent hover:text-foreground rounded p-1.5"
          :title="group.folders > 0 ? 'Open all folders' : 'Close all folders'"
          :aria-label="`${group.folders > 0 ? 'Open' : 'Close'} all ${group.title.toLowerCase()} folders`"
          @click="setAllFolders(group.kind, group.refs, group.folders === 0)"
        >
          <component :is="group.folders > 0 ? FolderOpen : Folder" class="size-3.5" />
        </button>
        <button
          v-if="group.refs.length"
          class="text-muted-foreground hover:bg-accent hover:text-foreground rounded p-1.5"
          :title="group.hidden === group.refs.length ? 'Show all' : 'Hide all'"
          :aria-label="`${group.hidden === group.refs.length ? 'Show' : 'Hide'} all ${group.title.toLowerCase()}`"
          @click="emit('toggleKind', group.kind)"
        >
          <component :is="group.hidden === group.refs.length ? EyeOff : Eye" class="size-3.5" />
        </button>
      </div>

      <ul v-if="!collapsed[group.kind] && group.refs.length" class="pb-1">
        <li
          v-for="row in group.rows"
          :key="row.kind === 'folder' ? `d:${row.path}` : row.ref.fullName"
          class="hover:bg-accent group flex items-center rounded pr-1"
          :class="{
            'opacity-50':
              row.kind === 'ref'
                ? tab.hidden.has(row.ref.fullName)
                : hiddenIn(row.refs) === row.refs.length,
          }"
          :style="indent(row.depth)"
          @contextmenu.prevent="
            row.kind === 'ref' && emit('refMenu', row.ref, $event.clientX, $event.clientY)
          "
        >
          <!-- A folder -->
          <template v-if="row.kind === 'folder'">
            <button
              class="flex min-w-0 flex-1 items-center gap-1.5 py-1 text-left"
              :aria-expanded="!row.collapsed"
              :title="`${row.path} (${row.refs.length})`"
              @click="toggleFolder(group.kind, row.path)"
            >
              <component :is="row.collapsed ? ChevronRight : ChevronDown" class="size-3 shrink-0" />
              <component
                :is="row.collapsed ? Folder : FolderOpen"
                class="text-muted-foreground size-3.5 shrink-0"
              />
              <span class="truncate">{{ row.name }}</span>
              <span
                v-if="row.collapsed && row.hasHead"
                class="bg-primary size-1.5 shrink-0 rounded-full"
                title="HEAD is in this folder"
              />
              <span class="text-muted-foreground ml-auto pl-1 text-xs">
                {{ row.refs.length - hiddenIn(row.refs) }}/{{ row.refs.length }}
              </span>
            </button>
            <button
              class="text-muted-foreground hover:text-foreground rounded p-1 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
              :class="{ 'opacity-100': hiddenIn(row.refs) === row.refs.length }"
              :title="
                hiddenIn(row.refs) === row.refs.length
                  ? 'Show this folder in the graph'
                  : 'Hide this folder from the graph'
              "
              :aria-label="`${hiddenIn(row.refs) === row.refs.length ? 'Show' : 'Hide'} ${row.path}`"
              @click="
                emit(
                  'toggleRefs',
                  row.refs.map((r) => r.fullName),
                )
              "
            >
              <component
                :is="hiddenIn(row.refs) === row.refs.length ? EyeOff : Eye"
                class="size-3.5"
              />
            </button>
          </template>

          <!-- A branch, a remote branch or a tag -->
          <template v-else>
            <button
              class="flex min-w-0 flex-1 items-center gap-1.5 py-1 text-left"
              :class="{ 'cursor-default': !rowOf.has(row.ref.fullName) }"
              :title="row.ref.fullName"
              @click="onRefClick(row.ref)"
            >
              <span class="truncate" :class="{ 'font-semibold': row.ref.isHead }">{{
                row.label
              }}</span>
              <span
                v-if="row.ref.isHead"
                class="bg-primary text-primary-foreground rounded px-1 text-[10px] leading-4 font-semibold"
              >
                HEAD
              </span>
            </button>
            <button
              v-if="laneOf.has(row.ref.fullName)"
              class="text-muted-foreground hover:text-foreground rounded p-1 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
              :class="{
                'text-foreground opacity-100': tab.highlightLane === laneOf.get(row.ref.fullName),
              }"
              :title="
                tab.highlightLane === laneOf.get(row.ref.fullName)
                  ? 'Remove the highlight'
                  : 'Highlight this branch'
              "
              :aria-label="`Highlight ${row.ref.name}`"
              :aria-pressed="tab.highlightLane === laneOf.get(row.ref.fullName)"
              @click="emit('toggleHighlight', laneOf.get(row.ref.fullName)!)"
            >
              <Focus class="size-3.5" />
            </button>
            <button
              class="text-muted-foreground hover:text-foreground rounded p-1 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
              :class="{ 'opacity-100': tab.hidden.has(row.ref.fullName) }"
              :title="tab.hidden.has(row.ref.fullName) ? 'Show in graph' : 'Hide from graph'"
              :aria-label="`${tab.hidden.has(row.ref.fullName) ? 'Show' : 'Hide'} ${row.ref.name}`"
              @click="emit('toggleRef', row.ref.fullName)"
            >
              <component :is="tab.hidden.has(row.ref.fullName) ? EyeOff : Eye" class="size-3.5" />
            </button>
          </template>
        </li>
      </ul>
      <p v-else-if="!group.refs.length" class="text-muted-foreground px-7 py-1 text-xs">None</p>
    </section>
    <section v-if="tab.stashes.length" class="border-b py-1">
      <button
        class="hover:bg-accent flex w-full items-center gap-1.5 rounded px-2 py-1.5 text-left font-medium"
        :aria-expanded="!stashesCollapsed"
        @click="stashesCollapsed = !stashesCollapsed"
      >
        <component :is="stashesCollapsed ? ChevronRight : ChevronDown" class="size-3.5" />
        <Archive class="text-muted-foreground size-3.5" />
        Stashes
        <span class="text-muted-foreground ml-auto text-xs font-normal">
          {{ tab.stashes.length }}
        </span>
      </button>
      <ul v-if="!stashesCollapsed" class="pb-1">
        <li
          v-for="stash in tab.stashes"
          :key="stash.id"
          class="hover:bg-accent group rounded py-1 pr-1 pl-7"
        >
          <div class="truncate" :title="stash.message">{{ stash.message }}</div>
          <div class="text-muted-foreground flex items-center gap-1 text-xs">
            <span>{{ stash.reference }}</span>
            <span class="ml-auto flex gap-0.5">
              <button
                class="hover:bg-background rounded px-1.5 py-0.5 disabled:opacity-50"
                :disabled="tab.readOnly"
                title="Apply and keep the stash"
                @click="emit('stash', 'apply', stash)"
              >
                Apply
              </button>
              <button
                class="hover:bg-background rounded px-1.5 py-0.5 disabled:opacity-50"
                :disabled="tab.readOnly"
                title="Apply and remove the stash"
                @click="emit('stash', 'pop', stash)"
              >
                Pop
              </button>
              <button
                class="hover:bg-background text-destructive rounded px-1.5 py-0.5 disabled:opacity-50"
                :disabled="tab.readOnly"
                title="Remove the stash"
                @click="emit('stash', 'drop', stash)"
              >
                Drop
              </button>
            </span>
          </div>
        </li>
      </ul>
    </section>
  </nav>
</template>
