<script setup lang="ts">
import { Check, Monitor, Moon, Palette, Sun } from "@lucide/vue";
import { computed, ref, watch } from "vue";
import ThemeEditor from "@/components/ThemeEditor.vue";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Textarea } from "@/components/ui/textarea";
import { useSettings } from "@/composables/useSettings";
import { type ThemeMode, useTheme } from "@/composables/useTheme";
import { exportTheme, type Theme } from "@/lib/theme/types";

const { open, closeSettings } = useSettings();
const { mode, setMode, themes, themeId, setTheme, createFrom, removeCustom, importTheme, dark } =
  useTheme();

/** Only one section for now; the window is laid out for more. */
const SECTIONS = [{ id: "appearance", label: "Appearance", icon: Palette }] as const;
const section = ref<(typeof SECTIONS)[number]["id"]>("appearance");

const MODES: Array<{ mode: ThemeMode; label: string; icon: typeof Sun }> = [
  { mode: "system", label: "System", icon: Monitor },
  { mode: "light", label: "Light", icon: Sun },
  { mode: "dark", label: "Dark", icon: Moon },
];

/** The theme being edited, or `null` for the list. */
const editing = ref<Theme | null>(null);
const confirmDelete = ref(false);
const importing = ref(false);
const importText = ref("");
const message = ref("");
const exported = ref("");

const selected = computed(() => themes.value.find((t) => t.id === themeId.value));

watch(open, (isOpen) => {
  if (isOpen) return;
  editing.value = null;
  confirmDelete.value = false;
  importing.value = false;
  importText.value = "";
  message.value = "";
  exported.value = "";
});

function swatches(theme: Theme) {
  const v = dark.value ? theme.dark : theme.light;
  return { colors: v.colors, lanes: v.lanes.slice(0, 6) };
}

function customize() {
  if (!selected.value) return;
  editing.value = createFrom(selected.value.id);
}

function remove() {
  if (!selected.value) return;
  removeCustom(selected.value.id);
  confirmDelete.value = false;
}

function doImport() {
  const result = importTheme(importText.value);
  if ("error" in result) {
    message.value = result.error;
    return;
  }
  importing.value = false;
  importText.value = "";
  message.value = `Imported “${result.theme.name}”.`;
}

async function doExport() {
  if (!selected.value) return;
  const text = exportTheme(selected.value);
  exported.value = text;
  try {
    await navigator.clipboard.writeText(text);
    message.value = "Theme copied to the clipboard.";
  } catch {
    message.value = "Copy the text below.";
  }
}

function onOpenChange(value: boolean) {
  if (!value) closeSettings();
}
</script>

<template>
  <Dialog :open="open" @update:open="onOpenChange">
    <DialogContent class="max-h-[90vh] gap-0 overflow-hidden p-0 sm:max-w-4xl">
      <DialogTitle class="sr-only">Settings</DialogTitle>
      <DialogDescription class="sr-only">Application settings</DialogDescription>
      <div class="flex h-[80vh] min-h-0">
        <nav class="w-44 shrink-0 space-y-1 border-r p-3" aria-label="Settings sections">
          <h2 class="px-2 pb-2 text-sm font-semibold">Settings</h2>
          <button
            v-for="s in SECTIONS"
            :key="s.id"
            class="hover:bg-accent flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm"
            :class="{ 'bg-accent': section === s.id }"
            :aria-current="section === s.id ? 'page' : undefined"
            @click="section = s.id"
          >
            <component :is="s.icon" class="size-4" />
            {{ s.label }}
          </button>
        </nav>

        <div class="min-h-0 flex-1 overflow-y-auto p-5">
          <template v-if="section === 'appearance'">
            <!-- The editor of a custom theme -->
            <template v-if="editing">
              <h2 class="mb-3 text-base font-semibold">Edit the theme</h2>
              <ThemeEditor :theme="editing" @close="editing = null" />
            </template>

            <template v-else>
              <h2 class="text-base font-semibold">Appearance</h2>

              <fieldset class="mt-4">
                <legend class="text-sm font-medium">Mode</legend>
                <p class="text-muted-foreground mb-2 text-xs">
                  Each theme has a light and a dark variant. “System” follows your computer.
                </p>
                <div class="flex gap-2" role="radiogroup" aria-label="Mode">
                  <Button
                    v-for="m in MODES"
                    :key="m.mode"
                    size="sm"
                    role="radio"
                    :aria-checked="mode === m.mode"
                    :variant="mode === m.mode ? 'default' : 'outline'"
                    @click="setMode(m.mode)"
                  >
                    <component :is="m.icon" class="size-3.5" />
                    {{ m.label }}
                  </Button>
                </div>
              </fieldset>

              <section class="mt-6">
                <h3 class="text-sm font-medium">Theme</h3>
                <ul
                  class="mt-2 grid gap-2 sm:grid-cols-2 lg:grid-cols-3"
                  role="radiogroup"
                  aria-label="Theme"
                >
                  <li v-for="t in themes" :key="t.id">
                    <button
                      role="radio"
                      :aria-checked="t.id === themeId"
                      class="hover:bg-accent w-full rounded-lg border p-2 text-left"
                      :class="{ 'ring-ring ring-2': t.id === themeId }"
                      @click="setTheme(t.id)"
                    >
                      <span
                        class="mb-2 flex h-14 items-end gap-1 rounded-md border p-1.5"
                        :style="{
                          background: swatches(t).colors.background,
                          borderColor: swatches(t).colors.border,
                        }"
                        aria-hidden="true"
                      >
                        <span
                          class="h-2 w-6 rounded-sm"
                          :style="{ background: swatches(t).colors.primary }"
                        />
                        <span
                          class="h-2 w-4 rounded-sm"
                          :style="{ background: swatches(t).colors.foreground }"
                        />
                        <span class="ml-auto flex gap-0.5">
                          <span
                            v-for="(lane, i) in swatches(t).lanes"
                            :key="i"
                            class="size-2 rounded-full"
                            :style="{ background: lane }"
                          />
                        </span>
                      </span>
                      <span class="flex items-center gap-1.5 text-sm">
                        <span class="min-w-0 flex-1 truncate">{{ t.name }}</span>
                        <span v-if="!t.builtin" class="text-muted-foreground text-xs">Custom</span>
                        <Check v-if="t.id === themeId" class="size-3.5" />
                      </span>
                    </button>
                  </li>
                </ul>

                <div class="mt-3 flex flex-wrap gap-2">
                  <Button size="sm" variant="outline" @click="customize">
                    {{ selected?.builtin ? "Customize a copy" : "Duplicate" }}
                  </Button>
                  <Button
                    v-if="selected && !selected.builtin"
                    size="sm"
                    variant="outline"
                    @click="editing = selected"
                  >
                    Edit
                  </Button>
                  <Button size="sm" variant="outline" @click="doExport">Export (JSON)</Button>
                  <Button size="sm" variant="outline" @click="importing = !importing">
                    Import…
                  </Button>
                  <template v-if="selected && !selected.builtin">
                    <Button
                      v-if="!confirmDelete"
                      size="sm"
                      variant="outline"
                      class="text-destructive"
                      @click="confirmDelete = true"
                    >
                      Delete
                    </Button>
                    <template v-else>
                      <Button size="sm" variant="destructive" @click="remove">
                        Delete “{{ selected.name }}”
                      </Button>
                      <Button size="sm" variant="ghost" @click="confirmDelete = false">Keep</Button>
                    </template>
                  </template>
                </div>

                <p v-if="message" class="text-muted-foreground mt-2 text-sm" role="status">
                  {{ message }}
                </p>
                <Textarea
                  v-if="exported"
                  :model-value="exported"
                  readonly
                  class="mt-2 h-32 font-mono text-xs"
                  aria-label="Exported theme"
                />

                <div v-if="importing" class="mt-3 space-y-2">
                  <label class="block text-sm">
                    Paste the JSON of a theme
                    <Textarea
                      v-model="importText"
                      class="mt-1 h-32 font-mono text-xs"
                      spellcheck="false"
                    />
                  </label>
                  <div class="flex gap-2">
                    <Button size="sm" :disabled="!importText.trim()" @click="doImport"
                      >Import</Button
                    >
                    <Button size="sm" variant="ghost" @click="importing = false">Cancel</Button>
                  </div>
                </div>
              </section>
            </template>
          </template>
        </div>
      </div>
    </DialogContent>
  </Dialog>
</template>
