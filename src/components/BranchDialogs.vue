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
import { Textarea } from "@/components/ui/textarea";
import { type RepoTab, useWorkspace } from "@/composables/useWorkspace";

const props = defineProps<{ tab: RepoTab }>();
const workspace = useWorkspace();

const dialog = computed(() => props.tab.dialog);

// Form state, reset each time another question is asked.
const name = ref("");
const switchToIt = ref(true);
const annotated = ref(false);
const message = ref("");
const upstream = ref<string>("");
const deleteOnRemote = ref(false);

const remoteBranches = computed(() => props.tab.refs.filter((ref) => ref.kind === "remote"));

watch(dialog, (next, previous) => {
  if (!next || (next.type === previous?.type && next.type === "deleteBranch")) return;
  name.value = next.type === "renameBranch" ? next.name : "";
  switchToIt.value = true;
  annotated.value = false;
  deleteOnRemote.value = false;
  message.value = "";
  if (next.type === "upstream") {
    const own = remoteBranches.value.find((r) => r.name.endsWith(`/${next.branch}`));
    upstream.value = own?.name ?? remoteBranches.value[0]?.name ?? "";
  }
});

/** The problem to show inside the dialog (bad or taken names). */
const problem = computed(() => {
  const error = props.tab.writeError;
  return error && ["invalidName", "alreadyExists"].includes(error.kind) ? error.message : "";
});

const busy = computed(() => props.tab.busy);
// The push, pull, remote deletion (RemoteDialogs), stash and reset (HistoryDialogs) dialogs
// belong to other components.
const REMOTE_TYPES = [
  "push",
  "pullStrategy",
  "deleteRemoteBranch",
  "stash",
  "dropStash",
  "reset",
  "merge",
  "abortOperation",
  "createPullRequest",
];
const open = computed(() => dialog.value !== null && !REMOTE_TYPES.includes(dialog.value.type));

function close() {
  workspace.closeDialog(props.tab);
}

function submit() {
  const d = dialog.value;
  if (!d || busy.value) return;
  const tab = props.tab;
  switch (d.type) {
    case "createBranch":
      if (name.value.trim()) {
        void workspace.createBranch(tab, name.value.trim(), d.start?.id ?? null, switchToIt.value);
      }
      break;
    case "renameBranch":
      if (name.value.trim() && name.value.trim() !== d.name) {
        void workspace.renameBranch(tab, d.name, name.value.trim());
      }
      break;
    case "deleteBranch":
      void workspace.deleteBranch(
        tab,
        d.name,
        d.unmerged !== null,
        deleteOnRemote.value ? d.tracking : null,
      );
      break;
    case "createTag":
      if (name.value.trim() && (!annotated.value || message.value.trim())) {
        void workspace.createTag(
          tab,
          name.value.trim(),
          d.target?.id ?? null,
          annotated.value ? message.value : null,
        );
      }
      break;
    case "deleteTag":
      void workspace.deleteTag(tab, d.name);
      break;
    case "upstream":
      void workspace.setUpstream(tab, d.branch, upstream.value || null);
      break;
    case "dirtyCheckout":
      break;
  }
}

const SELECT_CLASS =
  "border-input bg-background focus-visible:ring-ring/50 w-full rounded-md border px-3 py-1.5 text-sm outline-none focus-visible:ring-2";
</script>

<template>
  <Dialog :open="open" @update:open="(value) => !value && close()">
    <DialogContent
      v-if="dialog && open"
      :class="dialog.type === 'dirtyCheckout' ? 'sm:max-w-xl' : undefined"
    >
      <!-- Create a branch -->
      <form v-if="dialog.type === 'createBranch'" class="space-y-4" @submit.prevent="submit">
        <DialogHeader>
          <DialogTitle>Create a branch</DialogTitle>
          <DialogDescription>
            From
            <strong>{{ dialog.start?.label ?? tab.headBranch ?? "the current commit" }}</strong>.
          </DialogDescription>
        </DialogHeader>
        <label class="block text-sm">
          Name
          <Input v-model="name" class="mt-1" placeholder="feature/my-change" autofocus />
        </label>
        <label class="flex items-center gap-2 text-sm">
          <input v-model="switchToIt" type="checkbox" /> Switch to the new branch
        </label>
        <p v-if="problem" class="text-destructive text-sm" role="alert">{{ problem }}</p>
        <DialogFooter>
          <Button type="button" variant="outline" @click="close">Cancel</Button>
          <Button type="submit" :disabled="!name.trim() || busy">Create</Button>
        </DialogFooter>
      </form>

      <!-- Rename -->
      <form v-else-if="dialog.type === 'renameBranch'" class="space-y-4" @submit.prevent="submit">
        <DialogHeader>
          <DialogTitle>Rename {{ dialog.name }}</DialogTitle>
          <DialogDescription>Only the local branch is renamed.</DialogDescription>
        </DialogHeader>
        <Input v-model="name" aria-label="New name" autofocus />
        <p v-if="problem" class="text-destructive text-sm" role="alert">{{ problem }}</p>
        <DialogFooter>
          <Button type="button" variant="outline" @click="close">Cancel</Button>
          <Button type="submit" :disabled="!name.trim() || name.trim() === dialog.name || busy">
            Rename
          </Button>
        </DialogFooter>
      </form>

      <!-- Delete a branch -->
      <form v-else-if="dialog.type === 'deleteBranch'" class="space-y-4" @submit.prevent="submit">
        <DialogHeader>
          <DialogTitle>Delete {{ dialog.name }}?</DialogTitle>
          <DialogDescription v-if="!dialog.unmerged">
            The branch is deleted, not its commits: they stay in the history as long as another
            branch or tag reaches them. You can undo this right after.
          </DialogDescription>
          <DialogDescription v-else>
            This branch has commits that no other branch has.
          </DialogDescription>
        </DialogHeader>
        <label v-if="dialog.tracking" class="flex items-start gap-2 text-sm">
          <input v-model="deleteOnRemote" type="checkbox" class="mt-1" />
          <span>
            Also delete <strong>{{ dialog.tracking }}</strong> on the remote. It is removed from the
            server for everyone who uses it, and Undo cannot bring it back. Without this, the branch
            stays on the server and comes back as <strong>{{ dialog.tracking }}</strong> at the next
            fetch.
          </span>
        </label>
        <pre
          v-if="dialog.unmerged"
          class="border-destructive/50 text-muted-foreground max-h-40 overflow-auto rounded border p-2 text-xs whitespace-pre-wrap"
          role="alert"
          >{{ dialog.unmerged }}</pre>
        <p v-if="dialog.unmerged" class="text-sm">
          If you delete it anyway, those commits are no longer reachable from any branch. You can
          still bring the branch back with <em>Undo</em> in the notice that follows.
        </p>
        <DialogFooter>
          <Button type="button" variant="outline" @click="close">Cancel</Button>
          <Button type="submit" variant="destructive" :disabled="busy">
            {{ dialog.unmerged ? "Delete anyway" : "Delete" }}
          </Button>
        </DialogFooter>
      </form>

      <!-- Create a tag -->
      <form v-else-if="dialog.type === 'createTag'" class="space-y-4" @submit.prevent="submit">
        <DialogHeader>
          <DialogTitle>Create a tag</DialogTitle>
          <DialogDescription>
            At <strong>{{ dialog.target?.label ?? "the current commit" }}</strong>.
          </DialogDescription>
        </DialogHeader>
        <label class="block text-sm">
          Name
          <Input v-model="name" class="mt-1" placeholder="v1.0.0" autofocus />
        </label>
        <label class="flex items-center gap-2 text-sm">
          <input v-model="annotated" type="checkbox" /> Annotated tag (with a message)
        </label>
        <Textarea
          v-if="annotated"
          v-model="message"
          placeholder="Message"
          aria-label="Tag message"
          class="min-h-20 text-sm"
        />
        <p v-if="problem" class="text-destructive text-sm" role="alert">{{ problem }}</p>
        <DialogFooter>
          <Button type="button" variant="outline" @click="close">Cancel</Button>
          <Button type="submit" :disabled="!name.trim() || (annotated && !message.trim()) || busy">
            Create
          </Button>
        </DialogFooter>
      </form>

      <!-- Delete a tag -->
      <form v-else-if="dialog.type === 'deleteTag'" class="space-y-4" @submit.prevent="submit">
        <DialogHeader>
          <DialogTitle>Delete the tag {{ dialog.name }}?</DialogTitle>
          <DialogDescription>
            Only the local tag is removed. You can undo this right after.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button type="button" variant="outline" @click="close">Cancel</Button>
          <Button type="submit" variant="destructive" :disabled="busy">Delete</Button>
        </DialogFooter>
      </form>

      <!-- Upstream -->
      <form v-else-if="dialog.type === 'upstream'" class="space-y-4" @submit.prevent="submit">
        <DialogHeader>
          <DialogTitle>Upstream of {{ dialog.branch }}</DialogTitle>
          <DialogDescription>
            The remote branch this branch is compared with, and pulls from and pushes to.
          </DialogDescription>
        </DialogHeader>
        <select v-model="upstream" :class="SELECT_CLASS" aria-label="Upstream branch">
          <option value="">No upstream</option>
          <option v-for="remote in remoteBranches" :key="remote.fullName" :value="remote.name">
            {{ remote.name }}
          </option>
        </select>
        <DialogFooter>
          <Button type="button" variant="outline" @click="close">Cancel</Button>
          <Button type="submit" :disabled="busy">Save</Button>
        </DialogFooter>
      </form>

      <!-- Local changes in the way of a checkout -->
      <div v-else-if="dialog.type === 'dirtyCheckout'" class="space-y-4">
        <DialogHeader>
          <DialogTitle>Your changes are in the way</DialogTitle>
          <DialogDescription>
            Switching would overwrite changes to these files. Nothing has been changed yet.
          </DialogDescription>
        </DialogHeader>
        <ul class="bg-muted max-h-40 overflow-auto rounded p-2 font-mono text-xs">
          <li v-for="file in dialog.files" :key="file" :title="file" class="truncate">
            {{ file }}
          </li>
          <li v-if="!dialog.files.length" class="text-muted-foreground">{{ dialog.message }}</li>
        </ul>
        <ul class="text-muted-foreground list-disc space-y-1 pl-5 text-sm">
          <li>
            <strong class="text-foreground">Stash and switch</strong>: your changes (untracked files
            included) are stashed, then applied again on the new branch. They may conflict; they
            stay in the stash if so.
          </li>
          <li>
            <strong class="text-foreground">Discard and switch</strong>: your changes are thrown
            away. A copy is kept, so you can undo it.
          </li>
        </ul>
        <DialogFooter class="gap-2 sm:flex-wrap">
          <Button variant="outline" @click="close">Cancel</Button>
          <Button
            variant="destructive"
            :disabled="busy"
            @click="workspace.checkout(tab, dialog.target, 'discard')"
          >
            Discard and switch
          </Button>
          <Button :disabled="busy" @click="workspace.checkout(tab, dialog.target, 'stash')">
            Stash and switch
          </Button>
        </DialogFooter>
      </div>
    </DialogContent>
  </Dialog>
</template>
