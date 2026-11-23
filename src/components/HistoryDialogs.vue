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
import type { ResetMode } from "@/lib/api";

const props = defineProps<{ tab: RepoTab }>();
const workspace = useWorkspace();

const TYPES = ["stash", "dropStash", "reset"];

const dialog = computed(() => {
  const current = props.tab.dialog;
  return current && TYPES.includes(current.type) ? current : null;
});

const message = ref("");
const includeUntracked = ref(true);
const keepIndex = ref(false);
const mode = ref<ResetMode>("mixed");

watch(dialog, (next) => {
  if (!next) return;
  message.value = "";
  includeUntracked.value = true;
  keepIndex.value = false;
  mode.value = "mixed";
});

const busy = computed(() => props.tab.busy);
const changes = computed(() => {
  const status = props.tab.status;
  if (!status) return 0;
  return new Set([...status.staged, ...status.unstaged].map((entry) => entry.path)).size;
});

/** Problem to show in the stash dialog (nothing to stash). */
const problem = computed(() => {
  const error = props.tab.writeError;
  return error && error.kind === "nothingToCommit" ? error.message : "";
});

const MODES: Array<{ mode: ResetMode; title: string; text: string }> = [
  {
    mode: "soft",
    title: "Soft",
    text: "Move the branch only. The commits you step over become staged changes.",
  },
  {
    mode: "mixed",
    title: "Mixed",
    text: "Move the branch and unstage. The files keep their content.",
  },
  {
    mode: "hard",
    title: "Hard",
    text: "Move the branch and make the files match. Uncommitted changes are discarded (a snapshot is kept for Undo).",
  },
];

function close() {
  workspace.closeDialog(props.tab);
}
</script>

<template>
  <Dialog :open="dialog !== null" @update:open="(value) => !value && close()">
    <DialogContent v-if="dialog">
      <!-- Stash the changes -->
      <form
        v-if="dialog.type === 'stash'"
        class="space-y-4"
        @submit.prevent="workspace.stashChanges(tab, message, includeUntracked, keepIndex)"
      >
        <DialogHeader>
          <DialogTitle>Stash your changes</DialogTitle>
          <DialogDescription>
            {{ changes }} changed {{ changes === 1 ? "file goes" : "files go" }} aside and the
            working tree becomes clean. Apply the stash later from the sidebar.
          </DialogDescription>
        </DialogHeader>
        <label class="block text-sm">
          Message (optional)
          <Input v-model="message" class="mt-1" placeholder="What is in it" autofocus />
        </label>
        <label class="flex items-center gap-2 text-sm">
          <input v-model="includeUntracked" type="checkbox" /> Include new (untracked) files
        </label>
        <label class="flex items-center gap-2 text-sm">
          <input v-model="keepIndex" type="checkbox" /> Keep what is staged in the working tree
        </label>
        <p v-if="problem" class="text-destructive text-sm" role="alert">{{ problem }}</p>
        <DialogFooter>
          <Button type="button" variant="outline" @click="close">Cancel</Button>
          <Button type="submit" :disabled="busy">Stash</Button>
        </DialogFooter>
      </form>

      <!-- Drop a stash -->
      <form
        v-else-if="dialog.type === 'dropStash'"
        class="space-y-4"
        @submit.prevent="workspace.dropStash(tab, dialog.stash)"
      >
        <DialogHeader>
          <DialogTitle>Drop {{ dialog.stash.reference }}?</DialogTitle>
          <DialogDescription>
            “{{ dialog.stash.message }}” is removed from the list. You can undo this right after.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button type="button" variant="outline" @click="close">Cancel</Button>
          <Button type="submit" variant="destructive" :disabled="busy">Drop</Button>
        </DialogFooter>
      </form>

      <!-- Reset -->
      <form
        v-else-if="dialog.type === 'reset'"
        class="space-y-4"
        @submit.prevent="workspace.reset(tab, dialog.target, mode)"
      >
        <DialogHeader>
          <DialogTitle
            >Reset {{ tab.headBranch ?? "HEAD" }} to {{ dialog.target.label }}</DialogTitle
          >
          <DialogDescription>
            The current branch moves to this commit. You can undo it right after.
          </DialogDescription>
        </DialogHeader>
        <fieldset class="space-y-2">
          <legend class="sr-only">How to reset</legend>
          <label
            v-for="option in MODES"
            :key="option.mode"
            class="flex items-start gap-2 rounded border p-2 text-sm"
            :class="{ 'border-destructive': option.mode === 'hard' && mode === 'hard' }"
          >
            <input
              v-model="mode"
              type="radio"
              name="reset-mode"
              :value="option.mode"
              class="mt-1"
            />
            <span>
              <strong>{{ option.title }}</strong>
              <span class="text-muted-foreground block text-xs">{{ option.text }}</span>
            </span>
          </label>
        </fieldset>
        <p v-if="mode === 'hard' && changes > 0" class="text-destructive text-sm" role="alert">
          {{ changes }} file{{ changes === 1 ? "" : "s" }} with uncommitted changes will be
          discarded.
        </p>
        <DialogFooter>
          <Button type="button" variant="outline" @click="close">Cancel</Button>
          <Button
            type="submit"
            :variant="mode === 'hard' ? 'destructive' : 'default'"
            :disabled="busy"
          >
            Reset ({{ mode }})
          </Button>
        </DialogFooter>
      </form>
    </DialogContent>
  </Dialog>
</template>
