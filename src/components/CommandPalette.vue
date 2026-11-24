<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { formatAccelerator, type PaletteEntry, rankEntries } from "@/lib/commandPalette";

const props = defineProps<{
  entries: PaletteEntry[];
  /** Ids of the actions run lately, newest first. */
  recent: string[];
}>();
const emit = defineEmits<{ run: [id: string]; close: [] }>();

const query = ref("");
const selected = ref(0);
const input = ref<HTMLInputElement>();
const list = ref<HTMLElement>();

const mac = /Mac/i.test(navigator.userAgent);
const results = computed(() => rankEntries(props.entries, query.value, props.recent).slice(0, 50));

// A new query starts again from the best match.
watch(query, () => (selected.value = 0));

onMounted(() => input.value?.focus());

async function keepVisible() {
  await nextTick();
  list.value?.querySelector<HTMLElement>('[aria-selected="true"]')?.scrollIntoView?.({
    block: "nearest",
  });
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    emit("close");
  } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    const count = results.value.length;
    if (count === 0) return;
    selected.value = (selected.value + (event.key === "ArrowDown" ? 1 : count - 1)) % count;
    void keepVisible();
  } else if (event.key === "Enter") {
    event.preventDefault();
    const entry = results.value[selected.value];
    if (entry) emit("run", entry.id);
  }
}
</script>

<template>
  <div
    class="fixed inset-0 z-50 flex items-start justify-center bg-black/30 pt-[12vh]"
    @pointerdown.self="emit('close')"
  >
    <div
      class="bg-popover text-popover-foreground w-[34rem] max-w-[92vw] overflow-hidden rounded-lg border shadow-xl"
      role="dialog"
      aria-label="Command palette"
      @keydown="onKeydown"
    >
      <input
        ref="input"
        v-model="query"
        class="w-full border-b bg-transparent px-3 py-2.5 text-sm outline-none"
        placeholder="Type a command…"
        aria-label="Command"
        role="combobox"
        aria-expanded="true"
        aria-controls="palette-list"
        spellcheck="false"
        autocomplete="off"
      />
      <ul id="palette-list" ref="list" role="listbox" class="max-h-80 overflow-y-auto p-1">
        <li
          v-for="(entry, index) in results"
          :key="entry.id + entry.path"
          role="option"
          :aria-selected="index === selected"
          class="flex cursor-pointer items-center gap-3 rounded px-2 py-1.5 text-sm"
          :class="index === selected ? 'bg-accent' : ''"
          @pointermove="selected = index"
          @click="emit('run', entry.id)"
        >
          <span class="min-w-0 flex-1 truncate">{{ entry.path }}</span>
          <kbd v-if="entry.accelerator" class="text-muted-foreground shrink-0 text-xs">
            {{ formatAccelerator(entry.accelerator, mac) }}
          </kbd>
        </li>
        <li v-if="results.length === 0" class="text-muted-foreground px-2 py-3 text-sm">
          No command matches “{{ query }}”.
        </li>
      </ul>
    </div>
  </div>
</template>
