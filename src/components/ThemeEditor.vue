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
  FONT_SIZE,
  isFontList,
  SYNTAX_LABELS,
  SYNTAX_TOKENS,
  type SyntaxToken,
  syntaxOf,
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

const syntax = computed(() => syntaxOf(variant.value));

const warnings = computed(() => [
  ...PAIRS.flatMap(([a, b, label, min]) => {
    const ratio = contrastRatio(variant.value.colors[a], variant.value.colors[b]);
    return ratio < min
      ? [`${label}: contrast ${ratio.toFixed(1)}, ${min} or more is easier to read.`]
      : [];
  }),
  ...SYNTAX_TOKENS.filter((token) => token !== "comment").flatMap((token) => {
    const ratio = contrastRatio(syntax.value[token], variant.value.colors.background);
    return ratio < 3
      ? [
          `Code (${SYNTAX_LABELS[token].toLowerCase()}): contrast ${ratio.toFixed(1)}, 3 or more is easier to read.`,
        ]
      : [];
  }),
]);

const nameProblem = computed(() => (draft.value.name.trim() ? "" : "Give the theme a name."));
const fontProblem = computed(() =>
  [draft.value.fonts?.ui, draft.value.fonts?.code].some((f) => f && !isFontList(f))
    ? "Fonts are names separated by commas: letters, digits, spaces and quotes only."
    : "",
);

function setSyntax(token: SyntaxToken, value: string) {
  const v = draft.value[editing.value];
  v.syntax = { ...syntaxOf(v), [token]: value };
}

/** An empty field goes back to the default font of the application. */
function setFont(key: "ui" | "code", value: string) {
  const fonts = { ...draft.value.fonts };
  if (value.trim()) fonts[key] = value;
  else delete fonts[key];
  draft.value.fonts = Object.keys(fonts).length > 0 ? fonts : undefined;
}

function setSize(value: number | null) {
  const fonts = { ...draft.value.fonts };
  if (value === null) delete fonts.size;
  else fonts.size = value;
  draft.value.fonts = Object.keys(fonts).length > 0 ? fonts : undefined;
}

function setColor(token: ColorToken, value: string) {
  draft.value[editing.value].colors[token] = value;
}
function setLane(index: number, value: string) {
  draft.value[editing.value].lanes[index] = value;
}

function save() {
  if (nameProblem.value || fontProblem.value) return;
  draft.value.name = draft.value.name.trim();
  if (draft.value.fonts?.ui) draft.value.fonts.ui = draft.value.fonts.ui.trim();
  if (draft.value.fonts?.code) draft.value.fonts.code = draft.value.fonts.code.trim();
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

      <section class="space-y-1.5 sm:col-span-2">
        <h3 class="text-muted-foreground text-xs font-medium uppercase">Code in diffs</h3>
        <div class="grid gap-x-6 gap-y-1.5 sm:grid-cols-2">
          <ColorField
            v-for="token in SYNTAX_TOKENS"
            :key="token"
            :label="SYNTAX_LABELS[token]"
            :model-value="syntax[token]"
            @update:model-value="(value) => setSyntax(token, value)"
          />
        </div>
        <pre
          class="bg-card mt-1 overflow-x-auto rounded-md border p-2 font-mono text-xs"
          aria-label="Preview of the code colors"
        ><span :style="{ color: syntax.comment, fontStyle: 'italic' }">// Preview</span>
<span :style="{ color: syntax.keyword }">export function</span> <span :style="{ color: syntax.function }">total</span>(items: <span :style="{ color: syntax.type }">Item</span>[]) {
  <span :style="{ color: syntax.keyword }">return</span> items.length * <span :style="{ color: syntax.number }">3</span> + <span :style="{ color: syntax.string }">"ok"</span>.length;
}</pre>
      </section>

      <section class="space-y-2 sm:col-span-2">
        <h3 class="text-muted-foreground text-xs font-medium uppercase">Fonts</h3>
        <div class="grid gap-x-6 gap-y-2 sm:grid-cols-2">
          <label class="block text-sm">
            Interface font
            <Input
              :model-value="draft.fonts?.ui ?? ''"
              class="mt-1"
              placeholder="System default"
              maxlength="120"
              @update:model-value="(value) => setFont('ui', String(value))"
            />
          </label>
          <label class="block text-sm">
            Code font
            <Input
              :model-value="draft.fonts?.code ?? ''"
              class="mt-1"
              placeholder="System monospace"
              maxlength="120"
              @update:model-value="(value) => setFont('code', String(value))"
            />
          </label>
          <label class="block text-sm">
            Base size ({{ draft.fonts?.size ?? FONT_SIZE.default }} px)
            <span class="mt-1 flex items-center gap-2">
              <input
                type="range"
                class="w-full"
                :min="FONT_SIZE.min"
                :max="FONT_SIZE.max"
                step="1"
                :value="draft.fonts?.size ?? FONT_SIZE.default"
                @input="setSize(Number(($event.target as HTMLInputElement).value))"
              />
              <Button
                size="sm"
                variant="ghost"
                :disabled="draft.fonts?.size === undefined"
                @click="setSize(null)"
              >
                Default
              </Button>
            </span>
          </label>
        </div>
        <p v-if="fontProblem" class="text-destructive text-sm" role="alert">{{ fontProblem }}</p>
      </section>
    </div>

    <div class="flex justify-end gap-2 border-t pt-3">
      <Button variant="outline" @click="emit('close')">Cancel</Button>
      <Button :disabled="!!nameProblem || !!fontProblem" @click="save">Save</Button>
    </div>
  </div>
</template>
