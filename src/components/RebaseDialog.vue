<script setup lang="ts">
import { ArrowDown, ArrowUp, GripVertical } from "@lucide/vue";
import { computed, ref, watch } from "vue";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { type RepoTab, useWorkspace } from "@/composables/useWorkspace";
import type { TodoAction } from "@/lib/api";
import {
  ACTIONS,
  dropCount,
  isUntouched,
  moveRow,
  moveRowTo,
  previewOf,
  problem as listProblem,
  rowsOf,
  type TodoRow,
  toItems,
} from "@/lib/rebaseTodo";

const props = defineProps<{ tab: RepoTab }>();
const workspace = useWorkspace();

const dialog = computed(() => {
  const current = props.tab.dialog;
  return current?.type === "rebase" ? current : null;
});

const interactive = ref(false);
const autostash = ref(false);
const rows = ref<TodoRow[]>([]);

watch(dialog, (next) => {
  if (!next) return;
  interactive.value = next.interactive;
  autostash.value = false;
  rows.value = rowsOf(next.plan?.commits ?? []);
});

const plan = computed(() => dialog.value?.plan ?? null);
const busy = computed(() => props.tab.busy);
const dirty = computed(() => {
  const status = props.tab.status;
  return !!status && status.staged.length + status.unstaged.length > 0;
});
const problem = computed(() => (interactive.value ? listProblem(rows.value) : null));
const nothing = computed(() => plan.value !== null && plan.value.commits.length === 0);
const dropped = computed(() => dropCount(rows.value));

function setAction(index: number, action: TodoAction) {
  rows.value = rows.value.map((row, i) => (i === index ? { ...row, action } : row));
}
function setMessage(index: number, message: string) {
  rows.value = rows.value.map((row, i) => (i === index ? { ...row, message } : row));
}
function move(index: number, by: number) {
  rows.value = moveRow(rows.value, index, by);
}

const preview = computed(() => previewOf(rows.value));

// ---------- drag and drop ----------

const items = ref<HTMLElement[]>([]);
const dragging = ref<number | null>(null);
/** The gap the row would be dropped in (0 is above the first row). */
const dropAt = ref<number | null>(null);

function gapAt(clientY: number): number {
  const found = items.value.findIndex((el) => {
    const box = el.getBoundingClientRect();
    return clientY < box.top + box.height / 2;
  });
  return found === -1 ? rows.value.length : found;
}

function startDrag(index: number, event: PointerEvent) {
  (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  dragging.value = index;
  dropAt.value = index;
}
function dragOver(event: PointerEvent) {
  if (dragging.value !== null) dropAt.value = gapAt(event.clientY);
}
function endDrag(event: PointerEvent) {
  if (dragging.value === null) return;
  const to = event.type === "pointerup" ? gapAt(event.clientY) : dragging.value;
  rows.value = moveRowTo(rows.value, dragging.value, to);
  dragging.value = null;
  dropAt.value = null;
}

// ---------- keyboard: a letter sets the action of the focused row, Alt+arrows move it ----------

const KEYS: Record<string, TodoAction> = {
  p: "pick",
  r: "reword",
  e: "edit",
  s: "squash",
  f: "fixup",
  d: "drop",
};

function onRowKeydown(index: number, event: KeyboardEvent) {
  // Typing in the message field or using the selector keeps its own keys.
  if (event.target !== event.currentTarget) return;
  if (event.altKey && (event.key === "ArrowUp" || event.key === "ArrowDown")) {
    event.preventDefault();
    move(index, event.key === "ArrowUp" ? -1 : 1);
    return;
  }
  const action = event.metaKey || event.ctrlKey || event.altKey ? undefined : KEYS[event.key];
  if (action) {
    event.preventDefault();
    setAction(index, action);
  }
}

function close() {
  workspace.closeDialog(props.tab);
}

function submit() {
  const d = dialog.value;
  if (!d || busy.value || problem.value) return;
  // An interactive list that changes nothing is the same as a plain rebase.
  const plain = !interactive.value || (plan.value && isUntouched(rows.value, plan.value.commits));
  void workspace.rebase(props.tab, d.onto, plain ? null : toItems(rows.value), autostash.value);
}

const SELECT_CLASS =
  "border-input bg-background focus-visible:ring-ring/50 rounded-md border px-2 py-1 text-xs outline-none focus-visible:ring-2";
</script>

<template>
  <Dialog :open="dialog !== null" @update:open="(value) => !value && close()">
    <DialogContent
      v-if="dialog"
      class="max-h-[90vh] overflow-y-auto"
      :class="interactive && plan ? 'sm:max-w-5xl' : 'sm:max-w-2xl'"
    >
      <form class="space-y-4" @submit.prevent="submit">
        <DialogHeader>
          <DialogTitle>
            Rebase {{ tab.headBranch ?? "HEAD" }} onto
            {{ dialog.onto.length === 40 ? dialog.onto.slice(0, 7) : dialog.onto }}
          </DialogTitle>
          <DialogDescription v-if="nothing">
            There is nothing to replay: this branch is already on top of the target.
          </DialogDescription>
          <DialogDescription v-else-if="plan">
            {{ plan.commits.length }} commit{{ plan.commits.length === 1 ? "" : "s" }} will be
            replayed on top of the target. Their ids change.
          </DialogDescription>
          <DialogDescription v-else>
            The current branch is replayed on top of the target.
          </DialogDescription>
        </DialogHeader>

        <p v-if="plan && plan.pushed > 0" class="text-destructive text-sm" role="alert">
          {{ plan.pushed }} of these commits are already on the remote: after rewriting them you
          will have to push with <code>--force-with-lease</code>.
        </p>
        <p v-if="plan && plan.merges > 0" class="text-sm" role="note">
          {{ plan.merges }} merge commit{{ plan.merges === 1 ? "" : "s" }} in this range will be
          flattened: the commits they brought are replayed in order.
        </p>

        <label class="flex items-center gap-2 text-sm">
          <input v-model="interactive" type="checkbox" :disabled="nothing || !plan" />
          Edit the list of commits first
        </label>

        <div v-if="interactive && plan" class="grid gap-4 md:grid-cols-[3fr_2fr]">
          <ol class="space-y-1.5" aria-label="Commits to replay">
            <li
              v-for="(row, index) in rows"
              :key="row.id"
              :ref="
                (el) => {
                  if (el) items[index] = el as HTMLElement;
                }
              "
              tabindex="0"
              class="focus-visible:ring-ring/50 rounded border p-1.5 outline-none focus-visible:ring-2"
              :class="{
                'opacity-50': dragging === index,
                'border-t-primary border-t-2': dropAt === index && dragging !== null,
                'border-b-primary border-b-2':
                  dropAt === rows.length && index === rows.length - 1 && dragging !== null,
              }"
              @keydown="onRowKeydown(index, $event)"
            >
              <div class="flex items-center gap-1.5 text-sm">
                <button
                  type="button"
                  class="text-muted-foreground hover:text-foreground cursor-grab touch-none rounded p-0.5"
                  :aria-label="`Drag ${row.summary}`"
                  title="Drag to reorder (or focus the row and press Alt+Up or Alt+Down)"
                  @pointerdown.prevent="startDrag(index, $event)"
                  @pointermove="dragOver"
                  @pointerup="endDrag"
                  @pointercancel="endDrag"
                >
                  <GripVertical class="size-4" />
                </button>
                <select
                  :value="row.action"
                  :class="SELECT_CLASS"
                  :aria-label="`Action for ${row.summary}`"
                  @change="
                    setAction(index, ($event.target as HTMLSelectElement).value as TodoAction)
                  "
                >
                  <option v-for="a in ACTIONS" :key="a.action" :value="a.action" :title="a.help">
                    {{ a.label }}
                  </option>
                </select>
                <code class="text-muted-foreground text-xs">{{ row.id.slice(0, 7) }}</code>
                <span
                  class="min-w-0 flex-1 truncate"
                  :class="{ 'line-through opacity-60': row.action === 'drop' }"
                  :title="row.summary"
                >
                  {{ row.summary }}
                </span>
                <Button
                  type="button"
                  size="icon"
                  variant="ghost"
                  class="size-6"
                  :disabled="index === 0"
                  :aria-label="`Move ${row.summary} up`"
                  @click="move(index, -1)"
                >
                  <ArrowUp class="size-3.5" />
                </Button>
                <Button
                  type="button"
                  size="icon"
                  variant="ghost"
                  class="size-6"
                  :disabled="index === rows.length - 1"
                  :aria-label="`Move ${row.summary} down`"
                  @click="move(index, 1)"
                >
                  <ArrowDown class="size-3.5" />
                </Button>
              </div>
              <Input
                v-if="row.action === 'reword' || row.action === 'squash'"
                :model-value="row.message"
                class="mt-1.5 h-8 text-xs"
                :placeholder="
                  row.action === 'reword' ? 'New message' : 'Combined message (empty: keep both)'
                "
                :aria-label="`Message for ${row.summary}`"
                @update:model-value="(value) => setMessage(index, String(value))"
              />
            </li>
          </ol>

          <section aria-label="After the rebase">
            <h3 class="mb-1.5 text-sm font-semibold">After</h3>
            <ol class="space-y-1.5">
              <li
                v-for="commit in preview"
                :key="commit.ids[0]"
                class="bg-muted/40 rounded border p-1.5 text-sm"
              >
                <div class="flex items-center gap-1.5">
                  <span class="min-w-0 flex-1 truncate" :title="commit.title">{{
                    commit.title
                  }}</span>
                  <span v-if="commit.reworded" class="text-muted-foreground text-xs">reworded</span>
                  <span v-if="commit.edit" class="text-muted-foreground text-xs">stops here</span>
                  <span v-if="commit.ids.length > 1" class="text-muted-foreground text-xs">
                    {{ commit.ids.length }} commits in one
                  </span>
                </div>
                <ul v-if="commit.extra.length" class="text-muted-foreground mt-0.5 text-xs">
                  <li v-for="line in commit.extra" :key="line" class="truncate">+ {{ line }}</li>
                </ul>
              </li>
              <li v-if="preview.length === 0" class="text-muted-foreground text-sm">
                Nothing is left: every commit is dropped.
              </li>
            </ol>
          </section>
        </div>

        <p v-if="interactive && plan" class="text-muted-foreground text-xs">
          {{ rows.length }} commit{{ rows.length === 1 ? "" : "s" }} → {{ preview.length }}. Focus a
          row, then press P, R, E, S, F or D to set its action.
        </p>

        <p v-if="problem" class="text-destructive text-sm" role="alert">{{ problem }}</p>
        <p v-else-if="interactive && dropped > 0" class="text-sm" role="note">
          {{ dropped }} commit{{ dropped === 1 ? "" : "s" }} and their changes will be left out.
        </p>

        <label class="flex items-center gap-2 text-sm">
          <input v-model="autostash" type="checkbox" />
          Stash my changes while rebasing{{ dirty ? " (you have some)" : "" }}
        </label>
        <p class="text-muted-foreground text-xs">
          If a commit conflicts, the rebase stops and you resolve it from the banner: continue, skip
          that commit, or abort and go back.
        </p>

        <DialogFooter>
          <Button type="button" variant="outline" @click="close">Cancel</Button>
          <Button type="submit" :disabled="busy || nothing || !!problem">Rebase</Button>
        </DialogFooter>
      </form>
    </DialogContent>
  </Dialog>
</template>
