<script setup lang="ts">
import { ChevronDown, ChevronUp, X } from "@lucide/vue";
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { type RepoTab, useWorkspace } from "@/composables/useWorkspace";
import type { ConflictFile } from "@/lib/api";
import {
  type Choice,
  compose,
  conflictsOf,
  hasMarkers,
  parseConflicts,
  resultLineOf,
  type Segment,
  unresolvedCount,
} from "@/lib/conflicts";

const props = defineProps<{ tab: RepoTab }>();
const workspace = useWorkspace();

const CODES: Record<string, string> = {
  UU: "changed on both sides",
  AA: "added on both sides",
  DD: "deleted on both sides",
  AU: "added by us",
  UA: "added by them",
  DU: "deleted by us, changed by them",
  UD: "changed by us, deleted by them",
};

const files = computed(() => props.tab.status?.conflicted ?? []);
const path = computed(() => props.tab.resolverPath ?? "");

const file = ref<ConflictFile | null>(null);
const loading = ref(false);
const error = ref("");
const segments = ref<Segment[]>([]);
const choices = ref<Record<number, Choice>>({});
/** What the user typed in the result; `null` while the result follows the choices. */
const manual = ref<string | null>(null);

const blocks = computed(() => conflictsOf(segments.value));
const result = computed(() => manual.value ?? compose(segments.value, choices.value));
const unresolved = computed(() => unresolvedCount(segments.value, choices.value));
const stillMarked = computed(() => hasMarkers(result.value));
const sideOnly = computed(
  () => !!file.value && (file.value.chooseSideOnly || blocks.value.length === 0),
);
const busy = computed(() => props.tab.busy || props.tab.readOnly);

/** The conflict being looked at (position among the blocks of the file). */
const current = ref(0);
/** Conflicts whose common ancestor is shown. */
const shownBase = ref<ReadonlySet<number>>(new Set());
const cells = new Map<number, HTMLElement>();
const resultBox = ref<HTMLElement>();

function setCell(id: number, el: unknown) {
  if (el instanceof HTMLElement) cells.set(id, el);
  else cells.delete(id);
}

function toggleBase(id: number) {
  const next = new Set(shownBase.value);
  if (!next.delete(id)) next.add(id);
  shownBase.value = next;
}

/** The file after this one in the list, where "next" goes when the last conflict is done. */
const nextFile = computed(() => {
  const at = files.value.findIndex((entry) => entry.path === path.value);
  return at === -1 ? null : (files.value[at + 1]?.path ?? null);
});

/** Scrolls the result to the conflict, unless the user wrote the result by hand. */
function followInResult() {
  const block = blocks.value[current.value];
  if (!block || manual.value !== null) return;
  const line = resultLineOf(segments.value, choices.value, block.id);
  const box = resultBox.value?.querySelector("textarea");
  // 16px is the line height of the text of the result.
  if (box && line !== null) box.scrollTop = Math.max(0, line * 16 - 32);
}

async function reveal() {
  await nextTick();
  const block = blocks.value[current.value];
  cells.get(block?.id ?? -1)?.scrollIntoView?.({ block: "center", behavior: "smooth" });
  followInResult();
}

/** Goes to a conflict; past the last one it goes to the next file, if there is one. */
function goTo(index: number) {
  if (index < 0) return;
  if (index >= blocks.value.length) {
    if (nextFile.value) workspace.openResolver(props.tab, nextFile.value);
    return;
  }
  current.value = index;
  void reveal();
}

function onKeydown(event: KeyboardEvent) {
  if (!file.value || sideOnly.value || blocks.value.length === 0) return;
  const target = event.target as HTMLElement | null;
  // Typing in the result, a field or a selector keeps its own keys.
  if (target?.closest("textarea, input, select, [contenteditable='true']")) return;
  if (event.altKey && (event.key === "ArrowUp" || event.key === "ArrowDown")) {
    event.preventDefault();
    goTo(current.value + (event.key === "ArrowUp" ? -1 : 1));
    return;
  }
  if (event.metaKey || event.ctrlKey || event.altKey) return;
  const keyed = KEY_CHOICES[event.key];
  const block = blocks.value[current.value];
  if (keyed !== undefined && block) {
    event.preventDefault();
    choose(block.id, keyed.choice);
  }
}

onMounted(() => window.addEventListener("keydown", onKeydown));
onBeforeUnmount(() => window.removeEventListener("keydown", onKeydown));

/** The side a block's text is taken from, for the highlight of its two cells. */
function takes(choice: Choice | undefined, side: "ours" | "theirs"): boolean {
  return choice === side || choice === "both" || choice === "bothReversed";
}

let request = 0;
async function load(target: string) {
  const mine = ++request;
  file.value = null;
  error.value = "";
  segments.value = [];
  choices.value = {};
  manual.value = null;
  current.value = 0;
  shownBase.value = new Set();
  if (!target) return;
  loading.value = true;
  try {
    const loaded = await workspace.conflictFile(props.tab, target);
    if (mine !== request) return;
    file.value = loaded;
    segments.value = parseConflicts(loaded.worktree ?? "");
    void reveal();
  } catch (e) {
    if (mine === request) error.value = e instanceof Error ? e.message : String(e);
  } finally {
    if (mine === request) loading.value = false;
  }
}

watch(
  () => [props.tab.resolverPath, files.value.map((f) => f.path).join("\n")],
  () => {
    if (props.tab.resolverPath === "" && files.value.length > 0) {
      workspace.openResolver(props.tab);
      return;
    }
    if (props.tab.resolverPath !== null && files.value.length === 0) {
      workspace.closeResolver(props.tab);
      return;
    }
    const current = props.tab.resolverPath ?? "";
    if (current !== file.value?.path) void load(current);
  },
  { immediate: true },
);

function choose(id: number, choice: Choice) {
  choices.value = { ...choices.value, [id]: choice };
  manual.value = null;
}

function chooseAll(choice: Choice) {
  choices.value = Object.fromEntries(blocks.value.map((b) => [b.id, choice]));
  manual.value = null;
}

function markResolved() {
  if (!file.value || stillMarked.value) return;
  void workspace.resolveFile(props.tab, file.value.path, result.value);
}

const CHOICES: Array<{ choice: Choice; label: string; key: string }> = [
  { choice: "ours", label: "Ours", key: "1" },
  { choice: "theirs", label: "Theirs", key: "2" },
  { choice: "both", label: "Both", key: "3" },
  { choice: "bothReversed", label: "Both, theirs first", key: "4" },
];
/** `0` takes a choice back. */
const KEY_CHOICES: Record<string, { choice: Choice }> = {
  ...Object.fromEntries(CHOICES.map((option) => [option.key, { choice: option.choice }])),
  "0": { choice: null },
};
</script>

<template>
  <div class="bg-background flex" aria-label="Conflict resolver">
    <aside class="w-56 shrink-0 overflow-y-auto border-r text-sm">
      <h2 class="px-3 py-2 font-medium">Conflicts ({{ files.length }})</h2>
      <ul>
        <li v-for="entry in files" :key="entry.path">
          <button
            class="hover:bg-accent w-full px-3 py-1.5 text-left"
            :class="{ 'bg-accent': entry.path === path }"
            :title="CODES[entry.code] ?? entry.code"
            @click="workspace.openResolver(tab, entry.path)"
          >
            <span class="block truncate">{{ entry.path }}</span>
            <span class="text-muted-foreground block text-xs">
              {{ CODES[entry.code] ?? entry.code }}
            </span>
          </button>
        </li>
      </ul>
    </aside>

    <section class="flex min-w-0 flex-1 flex-col">
      <header class="flex items-center gap-2 border-b px-3 py-2 text-sm">
        <strong class="min-w-0 flex-1 truncate">{{ file?.path ?? path }}</strong>
        <Button
          size="sm"
          variant="outline"
          :disabled="!file || busy"
          @click="workspace.resolveSide(tab, path, 'ours')"
        >
          Take ours
        </Button>
        <Button
          size="sm"
          variant="outline"
          :disabled="!file || busy"
          @click="workspace.resolveSide(tab, path, 'theirs')"
        >
          Take theirs
        </Button>
        <Button
          size="icon"
          variant="ghost"
          aria-label="Back to the graph"
          @click="workspace.closeResolver(tab)"
        >
          <X class="size-4" />
        </Button>
      </header>

      <p v-if="loading" class="text-muted-foreground p-4 text-sm">Reading the file…</p>
      <p v-else-if="error" class="text-destructive p-4 text-sm" role="alert">{{ error }}</p>

      <!-- Only a side can be chosen -->
      <div v-else-if="file && sideOnly" class="space-y-3 p-4 text-sm">
        <p>
          {{ CODES[file.code] ?? file.code }}.
          <template v-if="file.chooseSideOnly">
            This file is binary or too large to edit here: choose which version to keep.
          </template>
          <template v-else>
            There are no conflict markers to edit: keep one side, or the file as it is now.
          </template>
        </p>
        <p class="text-muted-foreground text-xs">
          “Ours” is the branch you are on, “theirs” is what is being merged in. A side that deleted
          the file deletes it.
        </p>
        <Button
          v-if="!file.chooseSideOnly && file.worktree !== null && !hasMarkers(file.worktree)"
          size="sm"
          :disabled="busy"
          @click="workspace.resolveFile(tab, file.path, file.worktree ?? '')"
        >
          Keep the file as it is
        </Button>
      </div>

      <!-- Blocks and result -->
      <template v-else-if="file">
        <div class="flex flex-wrap items-center gap-1.5 border-b px-3 py-1.5 text-xs">
          <Button
            size="sm"
            variant="outline"
            :disabled="current === 0"
            title="Previous conflict (Alt+Up)"
            @click="goTo(current - 1)"
          >
            <ChevronUp class="size-3.5" /> Previous
          </Button>
          <span class="text-muted-foreground tabular-nums">
            {{ current + 1 }} / {{ blocks.length }}
          </span>
          <Button
            size="sm"
            variant="outline"
            :disabled="current === blocks.length - 1 && !nextFile"
            title="Next conflict (Alt+Down)"
            @click="goTo(current + 1)"
          >
            <ChevronDown class="size-3.5" />
            {{ current === blocks.length - 1 ? "Next file" : "Next" }}
          </Button>
          <span class="text-muted-foreground ml-2">This one:</span>
          <Button
            v-for="option in CHOICES"
            :key="option.label"
            size="sm"
            :variant="choices[blocks[current]?.id] === option.choice ? 'default' : 'outline'"
            :title="`Key ${option.key}`"
            @click="choose(blocks[current].id, option.choice)"
          >
            {{ option.label }}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            title="Key 0"
            :disabled="(choices[blocks[current]?.id] ?? null) === null"
            @click="choose(blocks[current].id, null)"
          >
            Undo
          </Button>
          <span class="text-muted-foreground ml-2">All:</span>
          <Button size="sm" variant="outline" @click="chooseAll('ours')">Ours</Button>
          <Button size="sm" variant="outline" @click="chooseAll('theirs')">Theirs</Button>
          <Button size="sm" variant="outline" @click="chooseAll('both')">Both</Button>
        </div>

        <!-- Ours and theirs side by side: one scroll, so their blocks stay level -->
        <div class="min-h-0 flex-1 overflow-y-auto">
          <div
            class="bg-background text-muted-foreground sticky top-0 z-10 grid grid-cols-2 divide-x border-b text-xs"
          >
            <div class="px-2 py-1">
              Ours {{ blocks[0]?.oursLabel && `(${blocks[0].oursLabel})` }}
            </div>
            <div class="px-2 py-1">
              Theirs {{ blocks[0]?.theirsLabel && `(${blocks[0].theirsLabel})` }}
            </div>
          </div>
          <div class="grid grid-cols-2 divide-x font-mono text-xs">
            <template
              v-for="(segment, at) in segments"
              :key="segment.type === 'text' ? `t${at}` : `c${segment.id}`"
            >
              <template v-if="segment.type === 'text'">
                <pre class="overflow-x-auto px-2">{{ segment.text }}</pre>
                <pre class="overflow-x-auto px-2">{{ segment.text }}</pre>
              </template>
              <template v-else>
                <div
                  :ref="(el) => setCell(segment.id, el)"
                  class="cursor-pointer border-y px-2 py-1"
                  :class="[
                    takes(choices[segment.id], 'ours')
                      ? 'bg-emerald-500/15'
                      : (choices[segment.id] ?? null) === null
                        ? 'bg-amber-500/15'
                        : 'opacity-50',
                    blocks[current]?.id === segment.id ? 'ring-primary ring-2 ring-inset' : '',
                  ]"
                  @click="
                    current = blocks.findIndex((b) => b.id === segment.id);
                    choose(segment.id, 'ours');
                  "
                >
                  <div class="text-muted-foreground flex items-center gap-2 font-sans">
                    <span>Conflict {{ segment.id + 1 }}</span>
                    <button
                      v-if="segment.base !== null"
                      class="hover:text-foreground ml-auto underline"
                      @click.stop="toggleBase(segment.id)"
                    >
                      {{ shownBase.has(segment.id) ? "Hide base" : "Show base" }}
                    </button>
                  </div>
                  <pre class="overflow-x-auto">{{ segment.ours || "(nothing)" }}</pre>
                </div>
                <div
                  class="cursor-pointer border-y px-2 py-1"
                  :class="[
                    takes(choices[segment.id], 'theirs')
                      ? 'bg-emerald-500/15'
                      : (choices[segment.id] ?? null) === null
                        ? 'bg-amber-500/15'
                        : 'opacity-50',
                    blocks[current]?.id === segment.id ? 'ring-primary ring-2 ring-inset' : '',
                  ]"
                  @click="
                    current = blocks.findIndex((b) => b.id === segment.id);
                    choose(segment.id, 'theirs');
                  "
                >
                  <div class="text-muted-foreground font-sans">&nbsp;</div>
                  <pre class="overflow-x-auto">{{ segment.theirs || "(nothing)" }}</pre>
                </div>
                <div
                  v-if="segment.base !== null && shownBase.has(segment.id)"
                  class="bg-muted/40 col-span-2 border-b px-2 py-1"
                >
                  <div class="text-muted-foreground font-sans">Common ancestor</div>
                  <pre class="overflow-x-auto">{{ segment.base || "(nothing)" }}</pre>
                </div>
              </template>
            </template>
          </div>
        </div>

        <div class="flex h-2/5 min-h-40 flex-col border-t">
          <div class="flex items-center gap-2 px-3 py-1 text-xs">
            <strong>Result</strong>
            <span v-if="manual !== null" class="text-muted-foreground">(edited by hand)</span>
            <span v-if="stillMarked" class="text-destructive ml-auto" role="alert">
              {{
                unresolved > 0
                  ? `${unresolved} conflict${unresolved === 1 ? "" : "s"} left to decide`
                  : "conflict markers remain in the text"
              }}
            </span>
            <Button
              size="sm"
              class="ml-auto"
              :class="{ 'ml-2': stillMarked }"
              :disabled="stillMarked || busy"
              @click="markResolved"
            >
              Mark as resolved
            </Button>
          </div>
          <div ref="resultBox" class="flex min-h-0 flex-1 flex-col">
            <Textarea
              :model-value="result"
              class="min-h-0 flex-1 resize-none rounded-none border-0 border-t font-mono text-xs"
              aria-label="Result"
              spellcheck="false"
              @update:model-value="(value) => (manual = String(value))"
            />
          </div>
        </div>
      </template>
    </section>
  </div>
</template>
