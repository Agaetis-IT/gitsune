<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import type { MenuItem } from "@/lib/menus";

const props = defineProps<{
  items: MenuItem[];
  /** Window coordinates of the click. */
  x: number;
  y: number;
}>();
const emit = defineEmits<{ select: [id: string]; close: [] }>();

const root = ref<HTMLElement>();

// Keep the menu inside the window.
const position = computed(() => ({
  left: `${Math.max(4, Math.min(props.x, window.innerWidth - 270))}px`,
  top: `${Math.max(4, Math.min(props.y, window.innerHeight - props.items.length * 34 - 16))}px`,
}));

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
    class="bg-popover text-popover-foreground fixed z-50 w-64 rounded-md border p-1 text-sm shadow-md"
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
