<script setup lang="ts">
import { Download, FolderOpen, X } from "@lucide/vue";
import { computed, ref, watch } from "vue";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { type BlankTab, useWorkspace } from "@/composables/useWorkspace";
import { repoNameFromUrl } from "@/lib/cloneUrl";
import type { RecentRepo } from "@/lib/recents";

const props = defineProps<{
  recents: readonly RecentRepo[];
  error: string;
  busy: boolean;
  /** The blank tab this screen belongs to; its clone is shown here. */
  blank: BlankTab | null;
}>();
const emit = defineEmits<{ open: []; openRecent: [path: string]; forget: [path: string] }>();
const workspace = useWorkspace();

const PARENT_KEY = "gitsune.cloneParent";

function rememberedParent(): string {
  try {
    return localStorage.getItem(PARENT_KEY) ?? "";
  } catch {
    return "";
  }
}

const url = ref("");
const parent = ref(rememberedParent());
const name = ref("");
const nameTouched = ref(false);

// The folder is named after the repository, until the user chooses another name.
watch(url, (value) => {
  if (!nameTouched.value) name.value = repoNameFromUrl(value);
});

const cloning = computed(() => props.blank?.clone != null);
const canClone = computed(
  () =>
    !cloning.value &&
    !props.busy &&
    url.value.trim() !== "" &&
    parent.value.trim() !== "" &&
    name.value.trim() !== "",
);

async function browse() {
  const picked = await workspace.pickFolder("Choose the folder to clone into");
  if (picked) parent.value = picked;
}

function submit() {
  if (!canClone.value) return;
  try {
    localStorage.setItem(PARENT_KEY, parent.value.trim());
  } catch {
    // Not remembered: the folder is asked again next time.
  }
  void workspace.cloneRepository(url.value.trim(), parent.value.trim(), name.value.trim());
}
</script>

<template>
  <div class="h-full overflow-y-auto">
    <div class="flex min-h-full items-center justify-center p-8">
      <div class="w-full max-w-md">
        <h1 class="text-2xl font-semibold tracking-tight">Gitsune</h1>
        <p class="text-muted-foreground mt-1 text-sm">Browse the history of a git repository.</p>

        <Button class="mt-6" :disabled="busy || cloning" @click="emit('open')">
          <FolderOpen class="size-4" />
          Browse for a repository
        </Button>
        <p v-if="error" class="text-destructive mt-3 text-sm break-words" role="alert">
          {{ error }}
        </p>

        <section class="mt-8" aria-label="Clone a repository">
          <h2 class="text-muted-foreground mb-2 text-xs font-medium tracking-wide uppercase">
            Clone
          </h2>
          <form class="space-y-3 rounded-md border p-3" @submit.prevent="submit">
            <label class="block text-sm">
              Repository address
              <Input
                v-model="url"
                class="mt-1"
                placeholder="https://github.com/team/project.git"
                autocomplete="off"
                spellcheck="false"
                :disabled="cloning"
                aria-label="Repository address"
              />
            </label>
            <div class="text-sm">
              <label for="clone-parent">Clone into</label>
              <div class="mt-1 flex gap-2">
                <Input
                  id="clone-parent"
                  v-model="parent"
                  class="min-w-0 flex-1"
                  placeholder="Folder"
                  autocomplete="off"
                  spellcheck="false"
                  :disabled="cloning"
                />
                <Button type="button" variant="outline" :disabled="cloning" @click="browse">
                  Browse…
                </Button>
              </div>
            </div>
            <label class="block text-sm">
              Folder name
              <Input
                v-model="name"
                class="mt-1"
                autocomplete="off"
                spellcheck="false"
                :disabled="cloning"
                aria-label="Folder name"
                @input="nameTouched = true"
              />
            </label>
            <p v-if="blank?.cloneError" class="text-destructive text-sm break-words" role="alert">
              {{ blank.cloneError }}
            </p>
            <Button type="submit" :disabled="!canClone">
              <Download class="size-4" />
              {{ cloning ? "Cloning…" : "Clone" }}
            </Button>
          </form>
        </section>

        <section v-if="recents.length" class="mt-8">
          <h2 class="text-muted-foreground mb-2 text-xs font-medium tracking-wide uppercase">
            Recent
          </h2>
          <ul class="divide-y rounded-md border">
            <li
              v-for="repo in recents"
              :key="repo.path"
              class="hover:bg-accent group flex items-center"
            >
              <button
                class="min-w-0 flex-1 px-3 py-2 text-left"
                :disabled="busy || cloning"
                @click="emit('openRecent', repo.path)"
              >
                <span class="block truncate text-sm font-medium">{{ repo.name }}</span>
                <span class="text-muted-foreground block truncate text-xs">{{ repo.path }}</span>
              </button>
              <button
                class="text-muted-foreground hover:text-foreground mr-2 rounded p-1 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
                :aria-label="`Remove ${repo.name} from recent repositories`"
                title="Remove from recent"
                @click="emit('forget', repo.path)"
              >
                <X class="size-3.5" />
              </button>
            </li>
          </ul>
        </section>
      </div>
    </div>
  </div>
</template>
