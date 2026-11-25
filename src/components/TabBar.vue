<script setup lang="ts">
import { ArrowDown, CircleAlert, Plus, X } from "@lucide/vue";
import { nextTick, ref } from "vue";
import type { TabEntry } from "@/composables/useWorkspace";
import { landingIndex } from "@/lib/tabOrder";

const props = defineProps<{ entries: readonly TabEntry[]; activeId: string | null }>();
const emit = defineEmits<{
  activate: [id: string];
  close: [id: string];
  new: [];
  /** Move the tab to this index of the tab bar. */
  reorder: [id: string, to: number];
}>();

const items = ref<HTMLElement[]>([]);

// ---------- drag and drop (pointer events: the HTML5 drag can be taken over by the window) ----------

/** How far the pointer must move before a press becomes a drag, so a click stays a click. */
const DRAG_THRESHOLD = 5;

const pressed = ref<{ id: string; index: number; x: number } | null>(null);
const dragging = ref<string | null>(null);
/** The gap the tab would be dropped in (0 is before the first tab). */
const dropAt = ref<number | null>(null);
let suppressClick = false;

function gapAt(clientX: number): number {
  const found = items.value.findIndex((el) => {
    const box = el.getBoundingClientRect();
    return clientX < box.left + box.width / 2;
  });
  return found === -1 ? props.entries.length : found;
}

function onPointerDown(id: string, index: number, event: PointerEvent) {
  if (event.button !== 0) return;
  // The close button keeps its own click.
  if ((event.target as HTMLElement).closest("[data-close]")) return;
  (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  pressed.value = { id, index, x: event.clientX };
}

function onPointerMove(event: PointerEvent) {
  const press = pressed.value;
  if (!press) return;
  if (dragging.value === null && Math.abs(event.clientX - press.x) < DRAG_THRESHOLD) return;
  dragging.value = press.id;
  dropAt.value = gapAt(event.clientX);
}

function reset() {
  pressed.value = null;
  dragging.value = null;
  dropAt.value = null;
}

function onPointerUp() {
  const press = pressed.value;
  if (press && dragging.value !== null && dropAt.value !== null) {
    emit("reorder", press.id, landingIndex(press.index, dropAt.value));
    // The click that follows the drop is not a request to switch tabs.
    suppressClick = true;
    setTimeout(() => (suppressClick = false), 0);
  }
  reset();
}

function onClick(id: string) {
  if (suppressClick) return;
  emit("activate", id);
}

// ---------- keyboard: Alt+arrows move the focused tab ----------

async function onKeydown(id: string, index: number, event: KeyboardEvent) {
  if (!event.altKey || (event.key !== "ArrowLeft" && event.key !== "ArrowRight")) return;
  event.preventDefault();
  emit("reorder", id, index + (event.key === "ArrowLeft" ? -1 : 1));
  // Moving the tab can drop the focus: put it back so the next press moves it again.
  await nextTick();
  items.value.find((el) => el.dataset.id === id)?.focus();
}
</script>

<template>
  <div class="bg-muted/40 flex items-stretch border-b">
    <div class="flex min-w-0 flex-1 items-stretch overflow-x-auto" role="tablist">
      <button
        v-for="(entry, index) in entries"
        :key="entry.id"
        ref="items"
        role="tab"
        :data-id="entry.id"
        :aria-selected="entry.id === activeId"
        :aria-keyshortcuts="'Alt+ArrowLeft Alt+ArrowRight'"
        :title="'blank' in entry ? 'New tab' : entry.path"
        class="group relative flex max-w-52 min-w-28 touch-none items-center gap-2 border-r px-3 text-sm select-none"
        :class="[
          entry.id === activeId
            ? 'bg-background text-foreground'
            : 'text-muted-foreground hover:bg-background/60',
          { 'opacity-60': dragging === entry.id },
        ]"
        @pointerdown="onPointerDown(entry.id, index, $event)"
        @pointermove="onPointerMove"
        @pointerup="onPointerUp"
        @pointercancel="reset"
        @click="onClick(entry.id)"
        @keydown="onKeydown(entry.id, index, $event)"
        @auxclick.middle.prevent="emit('close', entry.id)"
      >
        <span v-if="dropAt === index" class="bg-primary absolute inset-y-0 left-0 w-0.5" />
        <span
          v-if="dropAt === entries.length && index === entries.length - 1"
          class="bg-primary absolute inset-y-0 right-0 w-0.5"
        />
        <span class="truncate" :class="{ italic: 'blank' in entry }">{{ entry.name }}</span>
        <template v-if="!('blank' in entry)">
          <span
            v-if="entry.incoming > 0"
            class="bg-primary/15 text-primary flex shrink-0 items-center rounded px-1 text-[10px] leading-4 font-semibold"
            :title="`${entry.incoming} commit${entry.incoming === 1 ? '' : 's'} to pull from the remotes`"
          >
            <ArrowDown class="size-2.5" />{{ entry.incoming }}
          </span>
          <CircleAlert
            v-if="entry.autoFetchFailed"
            class="text-muted-foreground size-3.5 shrink-0"
            title="The last automatic fetch failed. Fetch by hand to see why."
          />
        </template>
        <span
          role="button"
          data-close
          :aria-label="`Close ${entry.name}`"
          class="hover:bg-accent ml-auto rounded p-0.5 opacity-60 group-hover:opacity-100"
          @click.stop="emit('close', entry.id)"
        >
          <X class="size-3.5" />
        </span>
      </button>
      <button
        class="text-muted-foreground hover:bg-background/60 hover:text-foreground px-3"
        aria-label="New tab"
        title="New tab"
        @click="emit('new')"
      >
        <Plus class="size-4" />
      </button>
    </div>
  </div>
</template>
