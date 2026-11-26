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
import { type RepoTab, useWorkspace } from "@/composables/useWorkspace";

const props = defineProps<{ tab: RepoTab }>();
const workspace = useWorkspace();

const REMOTE_TYPES = ["push", "pullStrategy", "deleteRemoteBranch", "pushTags"];

const dialog = computed(() => {
  const current = props.tab.dialog;
  return current && REMOTE_TYPES.includes(current.type) ? current : null;
});

const remote = ref("");
const setUpstream = ref(false);
/** A force push asks to be confirmed before it runs. */
const confirmingForce = ref(false);

watch(dialog, (next) => {
  if (next?.type === "pushTags") remote.value = next.remote;
  if (next?.type !== "push") return;
  remote.value = next.remote;
  setUpstream.value = next.setUpstream;
  confirmingForce.value = false;
});

const busy = computed(() => props.tab.busy || props.tab.remote !== null);

function close() {
  workspace.closeDialog(props.tab);
}

/** Pushes the branch; a force push replaces what is on the remote, unless someone pushed since. */
function submitPush(forceWithLease = false) {
  const d = dialog.value;
  if (d?.type !== "push" || busy.value || !remote.value) return;
  void workspace.push(props.tab, {
    remote: remote.value,
    branch: d.branch || null,
    setUpstream: setUpstream.value,
    forceWithLease,
  });
}

function pullFirst() {
  close();
  void workspace.pull(props.tab);
}

const SELECT_CLASS =
  "border-input bg-background focus-visible:ring-ring/50 w-full rounded-md border px-3 py-1.5 text-sm outline-none focus-visible:ring-2";
</script>

<template>
  <!-- Hidden while the operation runs, so that its progress (bottom of the graph) is visible;
       it comes back if the operation fails, and is closed when it succeeds. -->
  <Dialog
    :open="dialog !== null && tab.remote === null"
    @update:open="(value) => !value && close()"
  >
    <DialogContent v-if="dialog" :class="dialog.type === 'push' ? 'sm:max-w-lg' : undefined">
      <!-- Push -->
      <form v-if="dialog.type === 'push'" class="space-y-4" @submit.prevent="submitPush()">
        <DialogHeader>
          <DialogTitle>Push {{ dialog.branch || "the current branch" }}</DialogTitle>
          <DialogDescription v-if="dialog.reason">{{ dialog.reason }}</DialogDescription>
          <DialogDescription v-else>Send the commits of this branch to a remote.</DialogDescription>
        </DialogHeader>
        <label class="block text-sm">
          Remote
          <select v-model="remote" :class="[SELECT_CLASS, 'mt-1']" aria-label="Remote">
            <option v-for="name in dialog.remotes" :key="name" :value="name">{{ name }}</option>
          </select>
        </label>
        <p v-if="dialog.remotes.length === 0" class="text-destructive text-sm" role="alert">
          This repository has no remote. Add one in a terminal with
          <code>git remote add</code>.
        </p>
        <label class="flex items-start gap-2 text-sm">
          <input v-model="setUpstream" type="checkbox" class="mt-1" />
          <span>
            Track it: make
            <strong class="break-all">{{ remote || "the remote" }}/{{ dialog.branch }}</strong> its
            upstream
          </span>
        </label>
        <p
          v-if="confirmingForce"
          class="border-destructive/50 text-destructive rounded-md border p-2 text-sm"
          role="alert"
        >
          This replaces <strong class="break-all">{{ remote }}/{{ dialog.branch }}</strong> with
          your branch. The commits that are only on the remote are lost for everyone who relies on
          them. It is refused if someone pushed since your last fetch.
        </p>
        <p v-else class="text-muted-foreground text-xs">
          A force push (<code>--force-with-lease</code>) replaces what is on the remote. It is
          refused if someone pushed since your last fetch.
        </p>
        <DialogFooter v-if="confirmingForce">
          <Button type="button" variant="outline" @click="confirmingForce = false">Back</Button>
          <Button
            type="button"
            variant="destructive"
            :disabled="!remote || busy"
            @click="submitPush(true)"
          >
            Yes, force push
          </Button>
        </DialogFooter>
        <DialogFooter v-else>
          <Button type="button" variant="outline" @click="close">Cancel</Button>
          <Button v-if="dialog.canForce" type="button" variant="outline" @click="pullFirst">
            Pull first
          </Button>
          <Button
            type="button"
            variant="destructive"
            :disabled="!remote || busy"
            @click="confirmingForce = true"
          >
            Force push
          </Button>
          <Button type="submit" :disabled="!remote || busy">Push</Button>
        </DialogFooter>
      </form>

      <!-- Push tags -->
      <form
        v-else-if="dialog.type === 'pushTags'"
        class="space-y-4"
        @submit.prevent="remote && !busy && workspace.pushTags(tab, remote, dialog.tag)"
      >
        <DialogHeader>
          <DialogTitle>
            {{ dialog.tag === null ? "Push all the tags" : `Push the tag ${dialog.tag}` }}
          </DialogTitle>
          <DialogDescription>
            {{
              dialog.tag === null
                ? "Sends every tag of this repository that the remote does not have."
                : "Sends this tag to a remote."
            }}
            A tag the remote already has at another commit is refused, never overwritten.
          </DialogDescription>
        </DialogHeader>
        <label class="block text-sm">
          Remote
          <select v-model="remote" :class="[SELECT_CLASS, 'mt-1']" aria-label="Remote">
            <option v-for="name in dialog.remotes" :key="name" :value="name">{{ name }}</option>
          </select>
        </label>
        <DialogFooter>
          <Button type="button" variant="outline" @click="close">Cancel</Button>
          <Button type="submit" :disabled="!remote || busy">Push</Button>
        </DialogFooter>
      </form>

      <!-- Pull needs a strategy -->
      <div v-else-if="dialog.type === 'pullStrategy'" class="space-y-4">
        <DialogHeader>
          <DialogTitle>Your branch and the remote have both moved</DialogTitle>
          <DialogDescription>
            Choose how to bring the remote commits in. Nothing was changed yet.
          </DialogDescription>
        </DialogHeader>
        <ul class="space-y-2 text-sm">
          <li><strong>Merge</strong>: keeps both histories and adds a merge commit.</li>
          <li><strong>Rebase</strong>: replays your commits on top of the remote ones.</li>
        </ul>
        <DialogFooter>
          <Button type="button" variant="outline" @click="close">Cancel</Button>
          <Button
            type="button"
            variant="outline"
            :disabled="busy"
            @click="workspace.pull(tab, 'rebase')"
          >
            Rebase
          </Button>
          <Button type="button" :disabled="busy" @click="workspace.pull(tab, 'merge')"
            >Merge</Button
          >
        </DialogFooter>
      </div>

      <!-- Delete a remote branch -->
      <form
        v-else-if="dialog.type === 'deleteRemoteBranch'"
        class="space-y-4"
        @submit.prevent="workspace.deleteRemoteBranch(tab, dialog.remote, dialog.branch)"
      >
        <DialogHeader>
          <DialogTitle>Delete {{ dialog.branch }} on {{ dialog.remote }}?</DialogTitle>
          <DialogDescription>
            The branch is removed from the server for everyone who uses it. Your local branch, if
            there is one, stays. This cannot be undone from here.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button type="button" variant="outline" @click="close">Cancel</Button>
          <Button type="submit" variant="destructive" :disabled="busy">Delete on the remote</Button>
        </DialogFooter>
      </form>
    </DialogContent>
  </Dialog>
</template>
