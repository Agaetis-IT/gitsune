<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { type RepoTab, useWorkspace } from "@/composables/useWorkspace";
import { defaultWorktreePath, flatten, folderName } from "@/lib/worktrees";

const props = defineProps<{ tab: RepoTab }>();
const workspace = useWorkspace();

const dialog = computed(() => {
  const current = props.tab.dialog;
  return current?.type === "createWorktree" || current?.type === "removeWorktree" ? current : null;
});

// ---------- creating ----------

const useNewBranch = ref(false);
const existing = ref("");
const newName = ref("");
const start = ref("");
const folder = ref("");
/** The folder was typed or chosen: it no longer follows the branch. */
const folderTouched = ref(false);
const openIt = ref(true);
const force = ref(false);

const branch = computed(() => (useNewBranch.value ? newName.value.trim() : existing.value));

watch(
  () => dialog.value?.type,
  () => {
    const d = dialog.value;
    force.value = false;
    if (d?.type !== "createWorktree") return;
    useNewBranch.value = d.branches.length === 0;
    existing.value = d.branch && d.branches.includes(d.branch) ? d.branch : (d.branches[0] ?? "");
    newName.value = d.branch && !d.branches.includes(d.branch) ? d.branch : "";
    start.value = "";
    folderTouched.value = false;
    openIt.value = true;
    folder.value = defaultWorktreePath(props.tab.path, branch.value);
  },
  { immediate: true },
);

// The proposed folder follows the branch until the user chooses one.
watch(branch, (name) => {
  if (!folderTouched.value) folder.value = defaultWorktreePath(props.tab.path, name);
});

async function browse() {
  const parent = await workspace.pickFolder();
  if (!parent) return;
  folder.value = `${parent.replace(/\/+$/, "")}/${folderName(
    props.tab.path,
  )}${flatten(branch.value) ? `-${flatten(branch.value)}` : ""}`;
  folderTouched.value = true;
}

const busy = computed(() => props.tab.busy);
const canCreate = computed(() => !busy.value && branch.value !== "" && folder.value.trim() !== "");

function submitCreate() {
  if (!canCreate.value) return;
  void workspace.createWorktree(props.tab, {
    path: folder.value,
    branch: branch.value,
    newBranch: useNewBranch.value,
    start: start.value,
    open: openIt.value,
  });
}

// ---------- removing ----------

function submitRemove() {
  const d = dialog.value;
  if (d?.type !== "removeWorktree" || busy.value) return;
  void workspace.removeWorktree(props.tab, d.worktree, force.value);
}

const SELECT_CLASS =
  "border-input bg-background focus-visible:ring-ring/50 w-full rounded-md border px-3 py-1.5 text-sm outline-none focus-visible:ring-2";
</script>

<template>
  <Dialog :open="dialog !== null" @update:open="(open) => !open && workspace.closeDialog(tab)">
    <DialogContent v-if="dialog" class="sm:max-w-lg">
      <!-- New working tree -->
      <form
        v-if="dialog.type === 'createWorktree'"
        class="space-y-4"
        @submit.prevent="submitCreate"
      >
        <DialogHeader>
          <DialogTitle>New working tree</DialogTitle>
          <DialogDescription>
            Check a branch out in another folder, so two branches can be worked on at once.
          </DialogDescription>
        </DialogHeader>
        <div class="flex gap-4 text-sm">
          <label class="flex items-center gap-1.5">
            <input
              v-model="useNewBranch"
              type="radio"
              :value="false"
              :disabled="dialog.branches.length === 0"
            />
            An existing branch
          </label>
          <label class="flex items-center gap-1.5">
            <input v-model="useNewBranch" type="radio" :value="true" /> A new branch
          </label>
        </div>
        <label v-if="!useNewBranch" class="block text-sm">
          Branch
          <select v-model="existing" :class="[SELECT_CLASS, 'mt-1']" aria-label="Branch">
            <option v-for="name in dialog.branches" :key="name" :value="name">{{ name }}</option>
          </select>
          <span class="text-muted-foreground mt-1 block text-xs">
            Only branches that no working tree has checked out are listed.
          </span>
        </label>
        <template v-else>
          <label class="block text-sm">
            Name of the new branch
            <Input v-model="newName" class="mt-1" aria-label="New branch name" />
          </label>
          <label class="block text-sm">
            Start from
            <Input
              v-model="start"
              class="mt-1"
              placeholder="The current commit"
              aria-label="Start from"
            />
          </label>
        </template>
        <div class="text-sm">
          <label for="worktree-folder">Folder</label>
          <div class="mt-1 flex gap-2">
            <Input
              id="worktree-folder"
              v-model="folder"
              aria-label="Folder"
              @input="folderTouched = true"
            />
            <Button type="button" variant="outline" @click="browse">Browse…</Button>
          </div>
        </div>
        <label class="flex items-center gap-2 text-sm">
          <input v-model="openIt" type="checkbox" /> Open it in a tab
        </label>
        <p v-if="dialog.error" class="text-destructive text-sm" role="alert">{{ dialog.error }}</p>
        <DialogFooter>
          <Button type="button" variant="outline" @click="workspace.closeDialog(tab)"
            >Cancel</Button
          >
          <Button type="submit" :disabled="!canCreate">Create</Button>
        </DialogFooter>
      </form>

      <!-- Remove a working tree -->
      <form v-else class="space-y-4" @submit.prevent="submitRemove">
        <DialogHeader>
          <DialogTitle>Remove {{ folderName(dialog.worktree.path) }}?</DialogTitle>
          <DialogDescription>
            The folder <code>{{ dialog.worktree.path }}</code> is deleted. The branch
            <strong>{{ dialog.worktree.branch ?? "(detached)" }}</strong> and its commits stay in
            the repository.
          </DialogDescription>
        </DialogHeader>
        <p v-if="dialog.worktree.dirty" class="text-destructive text-sm" role="alert">
          This working tree has uncommitted changes or untracked files. Removing it throws them
          away.
        </p>
        <p v-if="dialog.error" class="text-destructive text-sm" role="alert">{{ dialog.error }}</p>
        <label v-if="dialog.worktree.dirty || dialog.error" class="flex items-center gap-2 text-sm">
          <input v-model="force" type="checkbox" /> Remove it anyway (<code>--force</code>)
        </label>
        <DialogFooter>
          <Button type="button" variant="outline" @click="workspace.closeDialog(tab)"
            >Cancel</Button
          >
          <Button
            type="submit"
            variant="destructive"
            :disabled="busy || ((dialog.worktree.dirty ?? false) && !force)"
          >
            Remove
          </Button>
        </DialogFooter>
      </form>
    </DialogContent>
  </Dialog>
</template>
