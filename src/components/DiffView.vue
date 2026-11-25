<script setup lang="ts">
import { ArrowLeft, Loader } from "@lucide/vue";
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch } from "vue";
import { Button } from "@/components/ui/button";
import type { Blame, DiffHunk, FileChange, FileDiff } from "@/lib/api";
import { blameRows, isDarkCssColor, relativeTime } from "@/lib/blameRows";
import { canPickLines, sortedSelection, toggled, toggledAll } from "@/lib/lineSelection";
import { personColor } from "@/lib/metro/people";
import { toSplitRows } from "@/lib/diffRows";
import { escapeHtml, highlighterFor, type LineHighlighter } from "@/lib/highlight";
import type { DiffMode } from "@/composables/useWorkspace";

const props = defineProps<{
  file: FileChange;
  /** Identifies the commit, so the diff is reloaded when another commit shows the same path. */
  commitId: string;
  mode: DiffMode;
  load: (force: boolean) => Promise<FileDiff>;
  /** Changes when the content may have changed: the diff is read again without being cleared. */
  version?: number;
  /** A button in the header of each hunk (stage or unstage it). */
  hunkAction?: { label: string; run: (hunkId: string) => void; disabled?: boolean } | null;
  /**
   * A button in the header of a hunk that acts on the lines ticked in the margin (unified layout
   * only), e.g. "Stage lines".
   */
  lineAction?: {
    label: string;
    run: (hunkId: string, lines: number[]) => void;
    disabled?: boolean;
  } | null;
  /** Reads who wrote each line; when set, a *Blame* button swaps the diff for that view. */
  blame?: { load: () => Promise<Blame> } | null;
  /** Small label next to the path (e.g. "Staged changes"). */
  heading?: string;
}>();
const emit = defineEmits<{
  close: [];
  "update:mode": [mode: DiffMode];
  /** The author of a line of the blame was clicked: show that commit. */
  openCommit: [id: string];
}>();

/** Above this many lines the diff is shown without highlighting, to stay responsive. */
const HIGHLIGHT_LIMIT = 3000;

const diff = shallowRef<FileDiff | null>(null);
const loading = ref(false);
const error = ref("");
const highlighter = shallowRef<LineHighlighter | null>(null);
/** The blame view is shown instead of the diff (declared before the watchers that reset it). */
const showBlame = ref(false);
const blameData = shallowRef<Blame | null>(null);
let request = 0;

async function fetchDiff(force = false, silent = false) {
  const current = ++request;
  loading.value = true;
  error.value = "";
  if (!force && !silent) diff.value = null;
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
  () => {
    showBlame.value = false;
    blameData.value = null;
    void fetchDiff();
  },
  {
    immediate: true,
  },
);

// ---------- lines ticked in the margin ----------

const selection = ref<Record<string, ReadonlySet<number>>>({});
// A new diff (the file changed, a hunk was staged) starts without a selection.
watch(diff, () => (selection.value = {}));

const selectionOf = (hunk: DiffHunk): ReadonlySet<number> => selection.value[hunk.id] ?? new Set();
const pickable = (hunk: DiffHunk): boolean =>
  !!props.lineAction && props.mode === "unified" && canPickLines(hunk);

function tick(hunk: DiffHunk, index: number) {
  selection.value = { ...selection.value, [hunk.id]: toggled(selectionOf(hunk), index) };
}
function tickAll(hunk: DiffHunk) {
  selection.value = { ...selection.value, [hunk.id]: toggledAll(hunk, selectionOf(hunk)) };
}
function runLines(hunk: DiffHunk) {
  const lines = sortedSelection(selectionOf(hunk));
  if (lines.length > 0) props.lineAction?.run(hunk.id, lines);
}

// ---------- blame ----------

const blameLoading = ref(false);
const blameError = ref("");
const blameDark = ref(false);
const blameLines = computed(() => (blameData.value ? blameRows(blameData.value) : []));
const canBlame = computed(
  () =>
    !!props.blame &&
    !!diff.value &&
    !diff.value.binary &&
    !diff.value.tooLarge &&
    !diff.value.submodule &&
    props.file.status !== "deleted",
);

async function toggleBlame() {
  if (showBlame.value) {
    showBlame.value = false;
    return;
  }
  showBlame.value = true;
  if (blameData.value || !props.blame) return;
  blameLoading.value = true;
  blameError.value = "";
  try {
    const loaded = await props.blame.load();
    blameDark.value = isDarkCssColor(getComputedStyle(document.body).backgroundColor);
    if (loaded.lines.length <= HIGHLIGHT_LIMIT && !highlighter.value) {
      highlighter.value = await highlighterFor(props.file.path);
    }
    blameData.value = loaded;
  } catch (e) {
    blameError.value = e instanceof Error ? e.message : String(e);
  } finally {
    blameLoading.value = false;
  }
}

watch(
  () => props.version,
  () => fetchDiff(false, true),
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
          <span v-if="heading" class="text-foreground font-medium">{{ heading }} · </span>
          {{ STATUS_LABEL[file.status] }}
          <template v-if="diff && !diff.binary && !diff.tooLarge">
            ·
            <span class="text-emerald-600 dark:text-emerald-400">+{{ diff.additions }}</span>
            <span class="text-rose-600 dark:text-rose-400"> −{{ diff.deletions }}</span>
          </template>
        </p>
      </div>
      <Button
        v-if="canBlame"
        size="sm"
        :variant="showBlame ? 'default' : 'outline'"
        :aria-pressed="showBlame"
        title="Who wrote each line"
        @click="toggleBlame"
      >
        Blame
      </Button>
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

        <!-- Blame -->
        <div v-else-if="showBlame">
          <p v-if="blameLoading" class="text-muted-foreground flex items-center gap-2 p-4 text-sm">
            <Loader class="size-4 animate-spin" /> Reading the history of the file…
          </p>
          <p v-else-if="blameError" class="text-destructive p-4 text-sm" role="alert">
            {{ blameError }}
          </p>
          <table v-else class="w-full border-collapse font-mono text-xs">
            <tbody>
              <tr
                v-for="row in blameLines"
                :key="row.line"
                :class="{ 'border-t': row.first && row.line > 1 }"
              >
                <td
                  class="w-64 max-w-64 border-l-4 px-2 align-top font-sans"
                  :style="{ borderLeftColor: personColor(row.commit.author, blameDark).fill }"
                >
                  <button
                    v-if="row.first"
                    class="block w-full text-left"
                    :disabled="row.commit.uncommitted"
                    :title="`${row.commit.summary} (${row.commit.id.slice(0, 7)})`"
                    @click="emit('openCommit', row.commit.id)"
                  >
                    <span class="block truncate font-medium">{{ row.commit.author }}</span>
                    <span class="text-muted-foreground block truncate">
                      {{
                        row.commit.uncommitted ? "not committed yet" : relativeTime(row.commit.time)
                      }}
                      · {{ row.commit.summary }}
                    </span>
                  </button>
                </td>
                <td class="text-muted-foreground w-12 px-2 text-right select-none">
                  {{ row.line }}
                </td>
                <td class="pr-3 whitespace-pre">
                  <!-- html() escapes the text itself: see highlight.ts -->
                  <!-- eslint-disable-next-line vue/no-v-html -->
                  <span v-html="html(row.text)" />
                </td>
              </tr>
            </tbody>
          </table>
          <p v-if="blameData?.truncated" class="text-muted-foreground border-t p-3 text-xs">
            Only the first lines of this file are shown.
          </p>
        </div>

        <!-- Unified -->
        <table v-else-if="mode === 'unified'" class="w-full border-collapse font-mono text-xs">
          <template v-for="hunk in diff.hunks" :key="hunk.header + hunk.oldStart + hunk.newStart">
            <tbody>
              <tr class="bg-muted/60 text-muted-foreground">
                <td colspan="5" class="px-3 py-1">
                  <div class="flex items-center gap-2">
                    <input
                      v-if="pickable(hunk)"
                      type="checkbox"
                      :checked="
                        selectionOf(hunk).size > 0 &&
                        hunk.lines.every((l, i) => l.kind === 'context' || selectionOf(hunk).has(i))
                      "
                      :aria-label="`Select every change of ${hunk.header}`"
                      @change="tickAll(hunk)"
                    />
                    <span>{{ hunk.header }}</span>
                    <Button
                      v-if="lineAction && pickable(hunk) && selectionOf(hunk).size > 0"
                      size="sm"
                      class="ml-auto h-5 px-2 text-[11px]"
                      :disabled="lineAction.disabled"
                      @click="runLines(hunk)"
                    >
                      {{ lineAction.label }} ({{ selectionOf(hunk).size }})
                    </Button>
                    <Button
                      v-if="hunkAction"
                      size="sm"
                      variant="outline"
                      :class="
                        lineAction && pickable(hunk) && selectionOf(hunk).size > 0
                          ? 'h-5 px-2 text-[11px]'
                          : 'ml-auto h-5 px-2 text-[11px]'
                      "
                      :disabled="hunkAction.disabled"
                      @click="hunkAction.run(hunk.id)"
                    >
                      {{ hunkAction.label }}
                    </Button>
                  </div>
                </td>
              </tr>
              <tr v-for="(line, index) in hunk.lines" :key="index" :class="lineClass[line.kind]">
                <td v-if="lineAction" class="w-6 px-1 text-center select-none">
                  <input
                    v-if="pickable(hunk) && line.kind !== 'context'"
                    type="checkbox"
                    :checked="selectionOf(hunk).has(index)"
                    :aria-label="`Select the ${line.kind === 'add' ? 'added' : 'removed'} line ${line.newNo ?? line.oldNo}`"
                    @change="tick(hunk, index)"
                  />
                </td>
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
                <td colspan="4" class="px-3 py-1">
                  <div class="flex items-center gap-2">
                    <span>{{ hunk.header }}</span>
                    <Button
                      v-if="hunkAction"
                      size="sm"
                      variant="outline"
                      class="ml-auto h-5 px-2 text-[11px]"
                      :disabled="hunkAction.disabled"
                      @click="hunkAction.run(hunk.id)"
                    >
                      {{ hunkAction.label }}
                    </Button>
                  </div>
                </td>
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
