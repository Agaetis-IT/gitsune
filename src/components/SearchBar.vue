<script setup lang="ts">
import { ChevronDown, ChevronUp, Search, X } from "@lucide/vue";
import { computed, onBeforeUnmount, ref } from "vue";

const props = defineProps<{
  /** Query of the last search (used as the initial text). */
  query: string;
  total: number;
  /** Index of the current match, -1 when there is none. */
  current: number;
  loading: boolean;
  error: string;
}>();
const emit = defineEmits<{
  search: [query: string];
  step: [delta: 1 | -1];
  clear: [];
  /** Esc, or the close button: the box goes away. */
  close: [];
}>();

const text = ref(props.query);
const input = ref<HTMLInputElement>();
let timer: ReturnType<typeof setTimeout> | undefined;
let pending = false;

function sendNow() {
  clearTimeout(timer);
  pending = false;
  emit("search", text.value);
}

function onInput() {
  pending = true;
  clearTimeout(timer);
  timer = setTimeout(sendNow, 200);
}

function clear() {
  clearTimeout(timer);
  pending = false;
  text.value = "";
  emit("clear");
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Enter") {
    event.preventDefault();
    if (pending) sendNow();
    else emit("step", event.shiftKey ? -1 : 1);
  } else if (event.key === "Escape") {
    clear();
    input.value?.blur();
    emit("close");
  }
}

const status = computed(() => {
  if (!props.query.trim() && !text.value.trim()) return "";
  if (props.error) return "Search failed";
  if (props.loading) return "…";
  if (props.total === 0) return "No results";
  return `${props.current + 1} of ${props.total}`;
});

defineExpose({
  focus: () => {
    input.value?.focus();
    input.value?.select();
  },
});

onBeforeUnmount(() => clearTimeout(timer));
</script>

<template>
  <div
    class="bg-background focus-within:ring-ring/50 flex items-center gap-1 rounded-md border px-2 focus-within:ring-2"
  >
    <Search class="text-muted-foreground size-3.5 shrink-0" />
    <input
      ref="input"
      v-model="text"
      type="search"
      class="placeholder:text-muted-foreground h-7 w-44 min-w-0 bg-transparent text-xs outline-none [&::-webkit-search-cancel-button]:hidden"
      placeholder="Search message, author, id"
      aria-label="Search commits"
      @input="onInput"
      @keydown="onKeydown"
    />
    <span
      v-if="status"
      class="text-muted-foreground shrink-0 text-xs"
      :class="{ 'text-destructive': error }"
      :title="error"
    >
      {{ status }}
    </span>
    <button
      class="text-muted-foreground hover:text-foreground rounded p-0.5 disabled:opacity-30"
      :disabled="total === 0"
      aria-label="Previous match"
      title="Previous match (Shift+Enter)"
      @click="emit('step', -1)"
    >
      <ChevronUp class="size-3.5" />
    </button>
    <button
      class="text-muted-foreground hover:text-foreground rounded p-0.5 disabled:opacity-30"
      :disabled="total === 0"
      aria-label="Next match"
      title="Next match (Enter)"
      @click="emit('step', 1)"
    >
      <ChevronDown class="size-3.5" />
    </button>
    <button
      class="text-muted-foreground hover:text-foreground rounded p-0.5"
      aria-label="Close the search"
      title="Close (Esc)"
      @click="
        clear();
        emit('close');
      "
    >
      <X class="size-3.5" />
    </button>
  </div>
</template>
