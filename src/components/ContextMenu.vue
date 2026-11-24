<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { MenuItem } from "@/lib/menus";

const props = defineProps<{
  items: MenuItem[];
  /** Window coordinates of the click. */
  x: number;
  y: number;
}>();
const emit = defineEmits<{ select: [id: string]; close: [] }>();

const root = ref<HTMLElement>();

/** The height the menu really has (long labels wrap), measured once it is shown. */
const height = ref(0);
const MARGIN = 4;

// Keep the menu inside the window: moved up when it would run past the bottom, and scrolled
// when it is taller than the window.
const position = computed(() => ({
  left: `${Math.max(MARGIN, Math.min(props.x, window.innerWidth - 270))}px`,
  top: `${Math.max(MARGIN, Math.min(props.y, window.innerHeight - height.value - MARGIN))}px`,
  maxHeight: `${window.innerHeight - MARGIN * 2}px`,
}));

async function measure() {
  await nextTick();
  height.value = root.value?.offsetHeight ?? 0;
}
watch(() => props.items, measure);

function pick(item: MenuItem) {
  if (item.disabled) return;
  emit("select", item.id);
}

function onPointerDown(event: PointerEvent) {
  if (root.value && !root.value.contains(event.target as Node)) emit("close");
}
function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") emit("close");
}

onMounted(() => {
  void measure();
  document.addEventListener("pointerdown", onPointerDown, true);
  document.addEventListener("keydown", onKeydown);
  window.addEventListener("blur", () => emit("close"), { once: true });
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
    class="bg-popover text-popover-foreground fixed z-50 w-64 overflow-y-auto rounded-md border p-1 text-sm shadow-md"
    :style="position"
    @contextmenu.prevent
  >
    <template v-for="item in items" :key="item.id">
      <div v-if="item.separatorBefore" class="bg-border -mx-1 my-1 h-px" role="separator" />
      <button
        role="menuitem"
        class="flex w-full flex-col rounded px-2 py-1.5 text-left"
        :class="[
          item.disabled ? 'cursor-not-allowed opacity-50' : 'hover:bg-accent',
          item.danger && !item.disabled ? 'text-destructive' : '',
        ]"
        :aria-disabled="!!item.disabled"
        :title="item.disabled"
        @click="pick(item)"
      >
        {{ item.label }}
        <span v-if="item.disabled" class="text-muted-foreground text-xs">{{ item.disabled }}</span>
      </button>
    </template>
  </div>
</template>
