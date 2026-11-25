<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { isColor, splitAlpha } from "@/lib/theme/types";

const props = defineProps<{ modelValue: string; label: string }>();
const emit = defineEmits<{ "update:modelValue": [value: string] }>();

const parts = computed(() => splitAlpha(props.modelValue));
const text = ref(props.modelValue);
watch(
  () => props.modelValue,
  (value) => (text.value = value),
);

/** The picker only knows #rrggbb: a transparency (borders in dark themes) is kept. */
function pick(event: Event) {
  const rgb = (event.target as HTMLInputElement).value;
  emit("update:modelValue", `${rgb}${parts.value.alpha}`);
}

function typed() {
  const value = text.value.trim();
  if (isColor(value)) emit("update:modelValue", value.toLowerCase());
  else text.value = props.modelValue;
}
</script>

<template>
  <div class="flex items-center gap-2 text-sm">
    <input
      type="color"
      :value="parts.rgb"
      :aria-label="label"
      class="h-7 w-9 shrink-0 cursor-pointer rounded border bg-transparent p-0.5"
      @input="pick"
    />
    <span class="min-w-0 flex-1 truncate">{{ label }}</span>
    <input
      v-model="text"
      :aria-label="`${label} (hex)`"
      class="border-input bg-background w-24 rounded border px-1.5 py-0.5 font-mono text-xs"
      spellcheck="false"
      maxlength="9"
      @change="typed"
    />
  </div>
</template>
