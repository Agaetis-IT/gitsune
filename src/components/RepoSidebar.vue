<script setup lang="ts">
import {
  Archive,
  ChevronDown,
  ChevronRight,
  Cloud,
  Eye,
  EyeOff,
  Focus,
  GitBranch,
  Tag,
} from "@lucide/vue";
import { computed, ref } from "vue";
import type { RepoTab } from "@/composables/useWorkspace";
import type { RefInfo, RefKind, StashEntry } from "@/lib/api";

const props = defineProps<{ tab: RepoTab }>();
const emit = defineEmits<{
  toggleRef: [fullName: string];
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

const groups = computed(() =>
  GROUPS.map((group) => {
    const refs = props.tab.refs.filter((ref) => ref.kind === group.kind);
    const hidden = refs.filter((ref) => props.tab.hidden.has(ref.fullName)).length;
    return { ...group, refs, hidden };
  }),
);

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
          v-for="reference in group.refs"
          :key="reference.fullName"
          class="hover:bg-accent group flex items-center rounded pr-1 pl-7"
          :class="{ 'opacity-50': tab.hidden.has(reference.fullName) }"
          @contextmenu.prevent="emit('refMenu', reference, $event.clientX, $event.clientY)"
        >
          <button
            class="flex min-w-0 flex-1 items-center gap-1.5 py-1 text-left"
            :class="{ 'cursor-default': !rowOf.has(reference.fullName) }"
            :title="reference.fullName"
            @click="onRefClick(reference)"
          >
            <span class="truncate" :class="{ 'font-semibold': reference.isHead }">{{
              reference.name
            }}</span>
            <span
              v-if="reference.isHead"
              class="bg-primary text-primary-foreground rounded px-1 text-[10px] leading-4 font-semibold"
            >
              HEAD
            </span>
          </button>
          <button
            v-if="laneOf.has(reference.fullName)"
            class="text-muted-foreground hover:text-foreground rounded p-1 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
            :class="{
              'text-foreground opacity-100': tab.highlightLane === laneOf.get(reference.fullName),
            }"
            :title="
              tab.highlightLane === laneOf.get(reference.fullName)
                ? 'Remove the highlight'
                : 'Highlight this branch'
            "
            :aria-label="`Highlight ${reference.name}`"
            :aria-pressed="tab.highlightLane === laneOf.get(reference.fullName)"
            @click="emit('toggleHighlight', laneOf.get(reference.fullName)!)"
          >
            <Focus class="size-3.5" />
          </button>
          <button
            class="text-muted-foreground hover:text-foreground rounded p-1 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
            :class="{ 'opacity-100': tab.hidden.has(reference.fullName) }"
            :title="tab.hidden.has(reference.fullName) ? 'Show in graph' : 'Hide from graph'"
            :aria-label="`${tab.hidden.has(reference.fullName) ? 'Show' : 'Hide'} ${reference.name}`"
            @click="emit('toggleRef', reference.fullName)"
          >
            <component :is="tab.hidden.has(reference.fullName) ? EyeOff : Eye" class="size-3.5" />
          </button>
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
