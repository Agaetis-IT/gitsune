<script setup lang="ts">
import { TriangleAlert } from "@lucide/vue";
import { computed, onBeforeUnmount, ref, watch } from "vue";
import ColorField from "@/components/ColorField.vue";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useTheme } from "@/composables/useTheme";
import {
  cloneTheme,
  contrastRatio,
  type ColorToken,
  TOKEN_LABELS,
  type Theme,
} from "@/lib/theme/types";

const props = defineProps<{ theme: Theme }>();
const emit = defineEmits<{ close: [] }>();

const { setMode, preview, saveCustom, dark } = useTheme();

/** The copy being edited: shown live on the page, saved only on request. */
const draft = ref<Theme>(cloneTheme(props.theme));
const editing = ref<"light" | "dark">(dark.value ? "dark" : "light");

// Editing a variant shows it, so that every change can be seen.
watch(editing, (value) => setMode(value), { immediate: true });
watch(draft, (value) => preview(value), { deep: true, immediate: true });
onBeforeUnmount(() => preview(null));

const variant = computed(() => draft.value[editing.value]);

const GROUPS: Array<{ title: string; tokens: ColorToken[] }> = [
  { title: "Surfaces", tokens: ["background", "card", "popover", "muted", "secondary", "accent"] },
  {
    title: "Text",
    tokens: [
      "foreground",
      "card-foreground",
      "popover-foreground",
      "muted-foreground",
      "secondary-foreground",
      "accent-foreground",
    ],
  },
  { title: "Actions", tokens: ["primary", "primary-foreground", "destructive", "ring"] },
  { title: "Lines", tokens: ["border", "input"] },
];

/** Pairs worth checking: text on its background. */
const PAIRS: Array<[ColorToken, ColorToken, string, number]> = [
  ["foreground", "background", "Text on the background", 4.5],
  ["card-foreground", "card", "Text on panels", 4.5],
  ["muted-foreground", "background", "Muted text on the background", 3],
  ["primary-foreground", "primary", "Text on buttons", 3],
];

const warnings = computed(() =>
  PAIRS.flatMap(([a, b, label, min]) => {
    const ratio = contrastRatio(variant.value.colors[a], variant.value.colors[b]);
    return ratio < min
      ? [`${label}: contrast ${ratio.toFixed(1)}, ${min} or more is easier to read.`]
      : [];
  }),
);

const nameProblem = computed(() => (draft.value.name.trim() ? "" : "Give the theme a name."));

function setColor(token: ColorToken, value: string) {
  draft.value[editing.value].colors[token] = value;
}
function setLane(index: number, value: string) {
  draft.value[editing.value].lanes[index] = value;
}

function save() {
  if (nameProblem.value) return;
  draft.value.name = draft.value.name.trim();
  if (saveCustom(draft.value)) emit("close");
}
</script>

<template>
  <div class="space-y-4">
    <div class="flex flex-wrap items-end gap-3">
      <label class="block min-w-48 flex-1 text-sm">
        Name
        <Input v-model="draft.name" class="mt-1" maxlength="60" />
      </label>
      <label class="block w-40 text-sm">
        Corner radius ({{ draft.radius.toFixed(2) }} rem)
        <input
          v-model.number="draft.radius"
          type="range"
          min="0"
          max="1.5"
          step="0.05"
          class="mt-2 w-full"
        />
      </label>
      <div class="flex gap-1" role="group" aria-label="Variant being edited">
        <Button
          v-for="v in ['light', 'dark'] as const"
          :key="v"
          size="sm"
          :variant="editing === v ? 'default' : 'outline'"
          @click="editing = v"
        >
          {{ v === "light" ? "Light" : "Dark" }}
        </Button>
      </div>
    </div>
    <p v-if="nameProblem" class="text-destructive text-sm" role="alert">{{ nameProblem }}</p>

    <ul v-if="warnings.length" class="space-y-1 text-xs" role="status">
      <li
        v-for="warning in warnings"
        :key="warning"
        class="flex items-center gap-1.5 text-amber-600 dark:text-amber-400"
      >
        <TriangleAlert class="size-3.5 shrink-0" /> {{ warning }}
      </li>
    </ul>

    <div class="grid gap-x-6 gap-y-4 sm:grid-cols-2">
      <section v-for="group in GROUPS" :key="group.title" class="space-y-1.5">
        <h3 class="text-muted-foreground text-xs font-medium uppercase">{{ group.title }}</h3>
        <ColorField
          v-for="token in group.tokens"
          :key="token"
          :label="TOKEN_LABELS[token]"
          :model-value="variant.colors[token]"
          @update:model-value="(value) => setColor(token, value)"
        />
      </section>
      <section class="space-y-1.5 sm:col-span-2">
        <h3 class="text-muted-foreground text-xs font-medium uppercase">Graph lines</h3>
        <div class="grid gap-x-6 gap-y-1.5 sm:grid-cols-2">
          <ColorField
            v-for="(lane, index) in variant.lanes"
            :key="index"
            :label="`Line ${index + 1}`"
            :model-value="lane"
            @update:model-value="(value) => setLane(index, value)"
          />
          <ColorField
            label="Unnamed branches"
            :model-value="variant.anonymous"
            @update:model-value="(value) => (draft[editing].anonymous = value)"
          />
        </div>
      </section>
    </div>

    <div class="flex justify-end gap-2 border-t pt-3">
      <Button variant="outline" @click="emit('close')">Cancel</Button>
      <Button :disabled="!!nameProblem" @click="save">Save</Button>
    </div>
  </div>
</template>
