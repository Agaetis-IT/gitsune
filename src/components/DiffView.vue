<script setup lang="ts">
import { ArrowLeft, Loader } from "@lucide/vue";
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch } from "vue";
import { Button } from "@/components/ui/button";
import type { DiffHunk, FileChange, FileDiff } from "@/lib/api";
import { toSplitRows } from "@/lib/diffRows";
import { escapeHtml, highlighterFor, type LineHighlighter } from "@/lib/highlight";
import type { DiffMode } from "@/composables/useWorkspace";

const props = defineProps<{
  file: FileChange;
  /** Identifies the commit, so the diff is reloaded when another commit shows the same path. */
  commitId: string;
  mode: DiffMode;
  load: (force: boolean) => Promise<FileDiff>;
}>();
const emit = defineEmits<{ close: []; "update:mode": [mode: DiffMode] }>();

/** Above this many lines the diff is shown without highlighting, to stay responsive. */
const HIGHLIGHT_LIMIT = 3000;

const diff = shallowRef<FileDiff | null>(null);
const loading = ref(false);
const error = ref("");
const highlighter = shallowRef<LineHighlighter | null>(null);
let request = 0;

async function fetchDiff(force = false) {
  const current = ++request;
  loading.value = true;
  error.value = "";
  if (!force) diff.value = null;
  try {
    const result = await props.load(force);
    if (current !== request) return;
    const lineCount = result.hunks.reduce((n, h) => n + h.lines.length, 0);
    highlighter.value =
      lineCount > 0 && lineCount <= HIGHLIGHT_LIMIT ? await highlighterFor(result.path) : null;
    if (current !== request) return;
    diff.value = result;
  } catch (e) {
    if (current === request) error.value = e instanceof Error ? e.message : String(e);
  } finally {
    if (current === request) loading.value = false;
  }
}

watch(
  () => [props.commitId, props.file.path, props.file.oldPath],
  () => fetchDiff(),
  {
    immediate: true,
  },
);

const html = (text: string): string =>
  text === "" ? "&nbsp;" : highlighter.value ? highlighter.value(text) : escapeHtml(text);

const splitHunks = computed(() =>
  props.mode === "split" && diff.value
    ? diff.value.hunks.map((hunk: DiffHunk) => ({ hunk, rows: toSplitRows(hunk) }))
    : [],
);

const STATUS_LABEL: Record<FileChange["status"], string> = {
  added: "Added",
  modified: "Modified",
  deleted: "Deleted",
  renamed: "Renamed",
  copied: "Copied",
  typeChanged: "Type changed",
};

function formatSize(bytes: number | null): string {
  if (bytes === null) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MiB`;
}

const emptyMessage = computed(() => {
  const d = diff.value;
  if (!d || d.hunks.length > 0) return "";
  if (d.status === "added" && d.newSize === 0) return "Empty file.";
  if (d.status === "deleted" && d.oldSize === 0) return "Empty file deleted.";
  if (d.status === "renamed" || d.status === "copied") return "Renamed without content changes.";
  return "No content changes (the file mode may have changed).";
});

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") emit("close");
}
onMounted(() => document.addEventListener("keydown", onKeydown));
onBeforeUnmount(() => document.removeEventListener("keydown", onKeydown));

const lineClass = {
  add: "bg-emerald-500/15",
  delete: "bg-rose-500/15",
  context: "",
} as const;
const gutterClass = {
  add: "bg-emerald-500/25",
  delete: "bg-rose-500/25",
  context: "bg-muted/40",
} as const;
const marker = { add: "+", delete: "-", context: " " } as const;
</script>

<template>
  <section class="bg-background flex h-full flex-col" aria-label="File diff">
    <header class="flex flex-wrap items-center gap-x-3 gap-y-1 border-b px-3 py-2 text-sm">
      <Button size="sm" variant="outline" @click="emit('close')">
        <ArrowLeft class="size-4" />
        Graph
      </Button>
      <div class="min-w-0 flex-1">
        <p class="truncate font-mono text-xs" :title="file.path">
          <template v-if="file.oldPath">
            <span class="text-muted-foreground">{{ file.oldPath }}</span> →
          </template>
          {{ file.path }}
        </p>
        <p class="text-muted-foreground text-xs">
          {{ STATUS_LABEL[file.status] }}
          <template v-if="diff && !diff.binary && !diff.tooLarge">
            ·
            <span class="text-emerald-600 dark:text-emerald-400">+{{ diff.additions }}</span>
            <span class="text-rose-600 dark:text-rose-400"> −{{ diff.deletions }}</span>
          </template>
        </p>
      </div>
      <div class="flex gap-1" role="group" aria-label="Diff layout">
        <Button
          size="sm"
          :variant="mode === 'unified' ? 'default' : 'outline'"
          @click="emit('update:mode', 'unified')"
        >
          Unified
        </Button>
        <Button
          size="sm"
          :variant="mode === 'split' ? 'default' : 'outline'"
          @click="emit('update:mode', 'split')"
        >
          Side by side
        </Button>
      </div>
    </header>

    <div class="min-h-0 flex-1 overflow-auto">
      <p v-if="loading && !diff" class="text-muted-foreground flex items-center gap-2 p-4 text-sm">
        <Loader class="size-4 animate-spin" /> Loading the diff…
      </p>
      <p v-else-if="error" class="text-destructive p-4 text-sm" role="alert">{{ error }}</p>

      <template v-else-if="diff">
        <div v-if="diff.submodule" class="text-muted-foreground p-4 text-sm">
          Submodule: only the commit it points to changed.
        </div>
        <div v-else-if="diff.binary" class="text-muted-foreground p-4 text-sm">
          Binary file<template v-if="diff.oldSize !== null || diff.newSize !== null">
            ({{ formatSize(diff.oldSize) || "—" }} →
            {{ formatSize(diff.newSize) || "—" }})</template
          >. No preview.
        </div>
        <div v-else-if="diff.tooLarge" class="p-4 text-sm">
          <p>
            This file is large ({{ formatSize(Math.max(diff.oldSize ?? 0, diff.newSize ?? 0)) }}),
            so its diff was not computed.
          </p>
          <Button v-if="diff.canForce" class="mt-3" :disabled="loading" @click="fetchDiff(true)">
            Load anyway
          </Button>
          <p v-else class="text-muted-foreground mt-2">It is too large to be shown here.</p>
        </div>
        <p v-else-if="emptyMessage" class="text-muted-foreground p-4 text-sm">{{ emptyMessage }}</p>

        <!-- Unified -->
        <table v-else-if="mode === 'unified'" class="w-full border-collapse font-mono text-xs">
          <template v-for="hunk in diff.hunks" :key="hunk.header + hunk.oldStart + hunk.newStart">
            <tbody>
              <tr class="bg-muted/60 text-muted-foreground">
                <td colspan="4" class="px-3 py-1">{{ hunk.header }}</td>
              </tr>
              <tr v-for="(line, index) in hunk.lines" :key="index" :class="lineClass[line.kind]">
                <td
                  class="text-muted-foreground w-12 min-w-12 px-2 text-right select-none"
                  :class="gutterClass[line.kind]"
                >
                  {{ line.oldNo ?? "" }}
                </td>
                <td
                  class="text-muted-foreground w-12 min-w-12 px-2 text-right select-none"
                  :class="gutterClass[line.kind]"
                >
                  {{ line.newNo ?? "" }}
                </td>
                <td class="w-4 text-center select-none">{{ marker[line.kind] }}</td>
                <td class="pr-3 whitespace-pre">
                  <!-- html() escapes the text itself: see highlight.ts -->
                  <!-- eslint-disable-next-line vue/no-v-html -->
                  <span v-html="html(line.text)" />
                  <span v-if="line.noEol" class="text-muted-foreground italic">
                    ⏎ no newline at end of file</span
                  >
                </td>
              </tr>
            </tbody>
          </template>
        </table>

        <!-- Side by side -->
        <table v-else class="w-full table-fixed border-collapse font-mono text-xs">
          <template v-for="{ hunk, rows } in splitHunks" :key="hunk.header + hunk.oldStart">
            <tbody>
              <tr class="bg-muted/60 text-muted-foreground">
                <td colspan="4" class="px-3 py-1">{{ hunk.header }}</td>
              </tr>
              <tr v-for="(row, index) in rows" :key="index">
                <template v-for="side in ['left', 'right'] as const" :key="side">
                  <td
                    class="text-muted-foreground w-12 px-2 text-right select-none"
                    :class="row[side] ? gutterClass[row[side].kind] : 'bg-muted/20'"
                  >
                    {{ row[side]?.lineNo ?? "" }}
                  </td>
                  <td
                    class="pr-3 pl-1 align-top break-words whitespace-pre-wrap"
                    :class="[
                      row[side] ? lineClass[row[side].kind] : 'bg-muted/20',
                      side === 'left' ? 'border-r' : '',
                    ]"
                  >
                    <!-- html() escapes the text itself: see highlight.ts -->
                    <!-- eslint-disable-next-line vue/no-v-html -->
                    <span v-if="row[side]" v-html="html(row[side].text)" />
                  </td>
                </template>
              </tr>
            </tbody>
          </template>
        </table>

        <p v-if="diff.truncated" class="text-muted-foreground border-t p-3 text-xs">
          Only the first lines of this diff are shown.
        </p>
      </template>
    </div>
  </section>
</template>
