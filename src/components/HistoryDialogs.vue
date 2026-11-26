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
import { type CommitTarget, type RepoTab, useWorkspace } from "@/composables/useWorkspace";
import type { ResetMode } from "@/lib/api";
import { checkedOutElsewhere, folderName } from "@/lib/worktrees";

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
/** The branch a reset moves; empty for a detached HEAD. */
const picked = ref("");

const branches = computed(() =>
  props.tab.refs.filter((ref) => ref.kind === "local").map((ref) => ref.name),
);
/** A branch that is not there yet is created at the commit. */
const creating = computed(() => picked.value !== "" && !branches.value.includes(picked.value));
/** The reset moves HEAD: the checked-out branch, or a detached HEAD. */
const isCurrent = computed(() => picked.value === "" || picked.value === props.tab.headBranch);
/**
 * Another branch only has its ref moved, whatever the mode, so a hard reset is fine there. Not
 * when another working tree has it checked out: its files would be left behind.
 */
const elsewhere = computed(() =>
  isCurrent.value
    ? undefined
    : checkedOutElsewhere(props.tab.worktrees, props.tab.path, picked.value),
);
const hardAllowed = computed(() => elsewhere.value === undefined);

function submitReset(target: CommitTarget) {
  if (picked.value === "") void workspace.reset(props.tab, target, mode.value);
  else
    void workspace.resetBranch(
      props.tab,
      picked.value,
      target,
      creating.value ? "mixed" : mode.value,
    );
}

watch(dialog, (next) => {
  if (!next) return;
  picked.value = next.type === "reset" ? (next.branch ?? "") : "";
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

/** What a mode does; on a branch that is not checked out, hard only moves the branch. */
const modeText = (option: { mode: ResetMode; text: string }): string =>
  option.mode === "hard" && !isCurrent.value
    ? "Move the branch to this commit. It is not checked out, so there are no files to rewrite."
    : option.text;

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
        @submit.prevent="submitReset(dialog.target)"
      >
        <DialogHeader>
          <DialogTitle>Reset {{ picked || "HEAD" }} to {{ dialog.target.label }}</DialogTitle>
          <DialogDescription v-if="creating">
            {{ picked }} does not exist yet: it is created at this commit.
          </DialogDescription>
          <DialogDescription v-else>
            The branch moves to this commit. You can undo it right after.
          </DialogDescription>
        </DialogHeader>
        <label v-if="dialog.choose && branches.length > 0" class="block space-y-1 text-sm">
          <span>Branch</span>
          <select
            v-model="picked"
            class="border-input bg-background h-9 w-full rounded-md border px-3 text-sm"
          >
            <option v-for="name in branches" :key="name" :value="name">{{ name }}</option>
          </select>
        </label>
        <fieldset v-if="!creating" class="space-y-2">
          <legend class="sr-only">How to reset</legend>
          <label
            v-for="option in MODES"
            :key="option.mode"
            class="flex items-start gap-2 rounded border p-2 text-sm"
            :class="{
              'border-destructive': option.mode === 'hard' && mode === 'hard' && isCurrent,
            }"
            :aria-disabled="option.mode === 'hard' && !hardAllowed ? 'true' : undefined"
          >
            <input
              v-model="mode"
              type="radio"
              name="reset-mode"
              :value="option.mode"
              :disabled="option.mode === 'hard' && !hardAllowed"
              class="mt-1"
            />
            <span>
              <strong>{{ option.title }}</strong>
              <span class="text-muted-foreground block text-xs">{{ modeText(option) }}</span>
            </span>
          </label>
        </fieldset>
        <p v-if="elsewhere" class="text-muted-foreground text-sm">
          {{ picked }} is checked out in another working tree ({{ folderName(elsewhere.path) }}): a
          hard reset has to be made there.
        </p>
        <p
          v-if="mode === 'hard' && isCurrent && changes > 0"
          class="text-destructive text-sm"
          role="alert"
        >
          {{ changes }} file{{ changes === 1 ? "" : "s" }} with uncommitted changes will be
          discarded.
        </p>
        <DialogFooter>
          <Button type="button" variant="outline" @click="close">Cancel</Button>
          <Button
            type="submit"
            :variant="mode === 'hard' && !creating && isCurrent ? 'destructive' : 'default'"
            :disabled="busy || (mode === 'hard' && !hardAllowed)"
          >
            {{ creating ? "Create" : `Reset (${mode})` }}
          </Button>
        </DialogFooter>
      </form>
    </DialogContent>
  </Dialog>
</template>
