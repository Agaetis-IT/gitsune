<script setup lang="ts">
import { X } from "@lucide/vue";
import { computed, ref, watch } from "vue";
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

let request = 0;
async function load(target: string) {
  const mine = ++request;
  file.value = null;
  error.value = "";
  segments.value = [];
  choices.value = {};
  manual.value = null;
  if (!target) return;
  loading.value = true;
  try {
    const loaded = await workspace.conflictFile(props.tab, target);
    if (mine !== request) return;
    file.value = loaded;
    segments.value = parseConflicts(loaded.worktree ?? "");
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

const CHOICES: Array<{ choice: Choice; label: string }> = [
  { choice: "ours", label: "Ours" },
  { choice: "theirs", label: "Theirs" },
  { choice: "both", label: "Both" },
  { choice: "bothReversed", label: "Both, theirs first" },
];
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
        <div class="min-h-0 flex-1 space-y-3 overflow-y-auto p-3">
          <div class="flex flex-wrap items-center gap-2 text-xs">
            <span class="text-muted-foreground">All blocks:</span>
            <Button size="sm" variant="outline" @click="chooseAll('ours')">Ours</Button>
            <Button size="sm" variant="outline" @click="chooseAll('theirs')">Theirs</Button>
            <Button size="sm" variant="outline" @click="chooseAll('both')">Both</Button>
          </div>
          <article
            v-for="(block, index) in blocks"
            :key="block.id"
            class="rounded border"
            :class="{ 'border-destructive/60': (choices[block.id] ?? null) === null }"
          >
            <header class="flex flex-wrap items-center gap-1 border-b px-2 py-1 text-xs">
              <span class="font-medium">Conflict {{ index + 1 }} of {{ blocks.length }}</span>
              <span class="ml-auto flex gap-1">
                <Button
                  v-for="option in CHOICES"
                  :key="option.label"
                  size="sm"
                  :variant="choices[block.id] === option.choice ? 'default' : 'outline'"
                  @click="choose(block.id, option.choice)"
                >
                  {{ option.label }}
                </Button>
              </span>
            </header>
            <div class="grid grid-cols-2 divide-x text-xs">
              <div class="min-w-0">
                <div class="text-muted-foreground px-2 pt-1">
                  Ours {{ block.oursLabel && `(${block.oursLabel})` }}
                </div>
                <pre class="overflow-x-auto p-2">{{ block.ours || "(nothing)" }}</pre>
              </div>
              <div class="min-w-0">
                <div class="text-muted-foreground px-2 pt-1">
                  Theirs {{ block.theirsLabel && `(${block.theirsLabel})` }}
                </div>
                <pre class="overflow-x-auto p-2">{{ block.theirs || "(nothing)" }}</pre>
              </div>
            </div>
          </article>
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
          <Textarea
            :model-value="result"
            class="min-h-0 flex-1 resize-none rounded-none border-0 border-t font-mono text-xs"
            aria-label="Result"
            spellcheck="false"
            @update:model-value="(value) => (manual = String(value))"
          />
        </div>
      </template>
    </section>
  </div>
</template>
