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
import type { MergeMode } from "@/lib/api";

const props = defineProps<{ tab: RepoTab }>();
const workspace = useWorkspace();

const TYPES = ["merge", "abortOperation"];
const dialog = computed(() => {
  const current = props.tab.dialog;
  return current && TYPES.includes(current.type) ? current : null;
});

const mode = ref<MergeMode>("default");
const message = ref("");
watch(dialog, (next) => {
  if (!next) return;
  mode.value = "default";
  message.value = "";
});

const busy = computed(() => props.tab.busy);
const info = computed(() => (dialog.value?.type === "merge" ? dialog.value.info : null));

const MODES: Array<{ mode: MergeMode; title: string; text: string }> = [
  {
    mode: "default",
    title: "Fast-forward when possible",
    text: "Move the branch if nothing diverged, otherwise make a merge commit.",
  },
  {
    mode: "noFastForward",
    title: "Always a merge commit",
    text: "Keeps a visible junction in the graph, even when a fast-forward is possible.",
  },
  {
    mode: "squash",
    title: "Squash",
    text: "Stage the combined changes as one, without recording a merge. You commit them yourself.",
  },
  {
    mode: "fastForwardOnly",
    title: "Fast-forward only",
    text: "Refuse if the branches diverged.",
  },
];

const operationName = computed(() => {
  const kind = props.tab.status?.operation?.kind;
  return kind === "cherryPick" ? "cherry-pick" : (kind ?? "operation");
});

function close() {
  workspace.closeDialog(props.tab);
}
</script>

<template>
  <Dialog :open="dialog !== null" @update:open="(value) => !value && close()">
    <DialogContent v-if="dialog">
      <form
        v-if="dialog.type === 'merge'"
        class="space-y-4"
        @submit.prevent="workspace.merge(tab, dialog.target, mode, message)"
      >
        <DialogHeader>
          <DialogTitle>Merge {{ dialog.target }} into {{ tab.headBranch ?? "HEAD" }}</DialogTitle>
          <DialogDescription v-if="info?.upToDate">
            Everything in {{ dialog.target }} is already in this branch: there is nothing to merge.
          </DialogDescription>
          <DialogDescription v-else-if="info">
            {{ info.commits }} commit{{ info.commits === 1 ? "" : "s" }} to bring in.
            {{
              info.canFastForward
                ? "This branch has nothing of its own, so it can simply move forward."
                : "Both branches have commits the other lacks: a merge commit is needed."
            }}
          </DialogDescription>
          <DialogDescription v-else>Bring the commits of this branch into yours.</DialogDescription>
        </DialogHeader>
        <fieldset class="space-y-2">
          <legend class="sr-only">How to merge</legend>
          <label
            v-for="option in MODES"
            :key="option.mode"
            class="flex items-start gap-2 rounded border p-2 text-sm"
          >
            <input
              v-model="mode"
              type="radio"
              name="merge-mode"
              :value="option.mode"
              class="mt-1"
            />
            <span>
              <strong>{{ option.title }}</strong>
              <span class="text-muted-foreground block text-xs">{{ option.text }}</span>
            </span>
          </label>
        </fieldset>
        <label v-if="mode === 'default' || mode === 'noFastForward'" class="block text-sm">
          Message of the merge commit (optional)
          <Input v-model="message" class="mt-1" placeholder="Merge branch…" />
        </label>
        <p class="text-muted-foreground text-xs">
          If the branches conflict, the merge stays open so you can resolve it, then continue — or
          abort and go back.
        </p>
        <DialogFooter>
          <Button type="button" variant="outline" @click="close">Cancel</Button>
          <Button type="submit" :disabled="busy || info?.upToDate === true">Merge</Button>
        </DialogFooter>
      </form>

      <form v-else class="space-y-4" @submit.prevent="workspace.abortOperation(tab)">
        <DialogHeader>
          <DialogTitle>Abort the {{ operationName }}?</DialogTitle>
          <DialogDescription>
            The repository goes back to how it was before it started. What you resolved so far is
            lost.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button type="button" variant="outline" @click="close">Keep working</Button>
          <Button type="submit" variant="destructive" :disabled="busy">Abort</Button>
        </DialogFooter>
      </form>
    </DialogContent>
  </Dialog>
</template>
