<script setup lang="ts">
import { EyeOff, Focus } from "@lucide/vue";
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import type { Lane } from "@/lib/api";

const props = defineProps<{
  lane: Lane;
  /** Position in the graph area. */
  x: number;
  y: number;
  highlighted: boolean;
  /** The lane is a branch that can be hidden (it has a ref). */
  canHide: boolean;
}>();
const emit = defineEmits<{ highlight: []; hide: []; close: [] }>();

const root = ref<HTMLElement>();

const KIND_LABEL = {
  branch: "Branch",
  merged: "Merged branch (no ref left)",
  anonymous: "Unnamed line",
} as const;
const title = computed(() => props.lane.name ?? "Unnamed line");

function onPointerDown(event: PointerEvent) {
  if (root.value && !root.value.contains(event.target as Node)) emit("close");
}
function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") emit("close");
}

onMounted(() => {
  document.addEventListener("pointerdown", onPointerDown, true);
  document.addEventListener("keydown", onKeydown);
});
onBeforeUnmount(() => {
  document.removeEventListener("pointerdown", onPointerDown, true);
  document.removeEventListener("keydown", onKeydown);
});
</script>

<template>
  <div
    ref="root"
    role="menu"
    class="bg-popover text-popover-foreground absolute z-30 w-60 rounded-md border p-1 text-sm shadow-md"
    :style="{ left: `${x + 8}px`, top: `${y + 8}px` }"
  >
    <div class="px-2 py-1.5">
      <p class="truncate font-medium" :title="title">{{ title }}</p>
      <p class="text-muted-foreground text-xs">{{ KIND_LABEL[lane.kind] }}</p>
    </div>
    <button
      role="menuitem"
      class="hover:bg-accent flex w-full items-center gap-2 rounded px-2 py-1.5 text-left"
      @click="emit('highlight')"
    >
      <Focus class="size-4" />
      {{ highlighted ? "Remove the highlight" : "Highlight this branch" }}
    </button>
    <button
      v-if="canHide"
      role="menuitem"
      class="hover:bg-accent flex w-full items-center gap-2 rounded px-2 py-1.5 text-left"
      @click="emit('hide')"
    >
      <EyeOff class="size-4" />
      Hide this branch
    </button>
  </div>
</template>
