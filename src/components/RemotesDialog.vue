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

const props = defineProps<{ tab: RepoTab }>();
const workspace = useWorkspace();

const dialog = computed(() => {
  const current = props.tab.dialog;
  return current?.type === "remotes" ? current : null;
});

const busy = computed(() => props.tab.busy || props.tab.remote !== null);
const error = computed(() => props.tab.writeError?.message ?? "");

// The remote being edited, or asked to be removed (one at a time).
const editing = ref<string | null>(null);
const editName = ref("");
const editUrl = ref("");
const removing = ref<string | null>(null);

const newName = ref("");
const newUrl = ref("");
const fetchAfter = ref(true);

// A fresh dialog starts from a clean form.
watch(
  () => dialog.value === null,
  (closed) => {
    if (!closed) return;
    editing.value = null;
    removing.value = null;
    newName.value = "";
    newUrl.value = "";
    fetchAfter.value = true;
  },
);

function startEdit(name: string, url: string) {
  removing.value = null;
  editing.value = name;
  editName.value = name;
  editUrl.value = url;
}

/** Renames and re-addresses in turn: only what changed is sent. */
async function saveEdit(name: string, url: string) {
  const nextName = editName.value.trim();
  const nextUrl = editUrl.value.trim();
  if (nextUrl !== url && !(await workspace.setRemoteUrl(props.tab, name, nextUrl))) return;
  if (nextName !== name && !(await workspace.renameRemote(props.tab, name, nextName))) return;
  editing.value = null;
}

/** The remote-tracking branches that go away with a remote. */
function trackingCount(name: string): number {
  return props.tab.refs.filter((ref) => ref.kind === "remote" && ref.name.startsWith(`${name}/`))
    .length;
}

async function confirmRemove(name: string) {
  if (await workspace.removeRemote(props.tab, name)) removing.value = null;
}

const canAdd = computed(
  () => !busy.value && newName.value.trim() !== "" && newUrl.value.trim() !== "",
);

async function submitAdd() {
  if (!canAdd.value) return;
  if (await workspace.addRemote(props.tab, newName.value, newUrl.value, fetchAfter.value)) {
    newName.value = "";
    newUrl.value = "";
  }
}
</script>

<template>
  <Dialog :open="dialog !== null" @update:open="(value) => !value && workspace.closeDialog(tab)">
    <DialogContent v-if="dialog" class="sm:max-w-xl">
      <DialogHeader>
        <DialogTitle>Remotes</DialogTitle>
        <DialogDescription>
          The servers this repository fetches from and pushes to.
        </DialogDescription>
      </DialogHeader>

      <p v-if="dialog.remotes.length === 0" class="text-muted-foreground text-sm">
        This repository has no remote yet.
      </p>
      <ul v-else class="divide-y rounded-md border">
        <li v-for="remote in dialog.remotes" :key="remote.name" class="p-3 text-sm">
          <!-- Editing -->
          <form
            v-if="editing === remote.name"
            class="space-y-2"
            @submit.prevent="saveEdit(remote.name, remote.url)"
          >
            <label class="block">
              Name
              <Input v-model="editName" class="mt-1" aria-label="Remote name" autocomplete="off" />
            </label>
            <label class="block">
              Address
              <Input
                v-model="editUrl"
                class="mt-1"
                aria-label="Remote address"
                autocomplete="off"
                spellcheck="false"
              />
            </label>
            <div class="flex justify-end gap-2">
              <Button type="button" variant="outline" size="sm" @click="editing = null">
                Cancel
              </Button>
              <Button
                type="submit"
                size="sm"
                :disabled="busy || editName.trim() === '' || editUrl.trim() === ''"
              >
                Save
              </Button>
            </div>
          </form>

          <!-- Asking to remove -->
          <div v-else-if="removing === remote.name" class="space-y-2">
            <p>
              Remove <strong>{{ remote.name }}</strong>?
              <span class="text-muted-foreground block text-xs">
                {{ trackingCount(remote.name) }} remote-tracking
                {{ trackingCount(remote.name) === 1 ? "branch is" : "branches are" }}
                forgotten here. Nothing changes on the server.
              </span>
            </p>
            <div class="flex justify-end gap-2">
              <Button type="button" variant="outline" size="sm" @click="removing = null">
                Cancel
              </Button>
              <Button
                type="button"
                variant="destructive"
                size="sm"
                :disabled="busy"
                @click="confirmRemove(remote.name)"
              >
                Remove
              </Button>
            </div>
          </div>

          <!-- Showing -->
          <div v-else class="flex items-center gap-2">
            <div class="min-w-0 flex-1">
              <div class="font-medium">{{ remote.name }}</div>
              <div class="text-muted-foreground truncate font-mono text-xs" :title="remote.url">
                {{ remote.url }}
              </div>
            </div>
            <Button
              type="button"
              variant="outline"
              size="sm"
              :disabled="busy"
              :aria-label="`Edit ${remote.name}`"
              @click="startEdit(remote.name, remote.url)"
            >
              Edit
            </Button>
            <Button
              type="button"
              variant="outline"
              size="sm"
              :disabled="busy"
              :aria-label="`Remove ${remote.name}`"
              @click="
                removing = remote.name;
                editing = null;
              "
            >
              Remove
            </Button>
          </div>
        </li>
      </ul>

      <form class="space-y-2 border-t pt-4" @submit.prevent="submitAdd">
        <h3 class="text-sm font-medium">Add a remote</h3>
        <div class="flex gap-2">
          <Input
            v-model="newName"
            class="w-32 shrink-0"
            placeholder="Name"
            aria-label="New remote name"
            autocomplete="off"
            spellcheck="false"
          />
          <Input
            v-model="newUrl"
            class="min-w-0 flex-1"
            placeholder="https://github.com/team/project.git"
            aria-label="New remote address"
            autocomplete="off"
            spellcheck="false"
          />
        </div>
        <label class="flex items-center gap-2 text-sm">
          <input v-model="fetchAfter" type="checkbox" /> Fetch it after adding
        </label>
        <p v-if="error" class="text-destructive text-sm break-words" role="alert">{{ error }}</p>
        <DialogFooter>
          <Button type="button" variant="outline" @click="workspace.closeDialog(tab)">
            Close
          </Button>
          <Button type="submit" :disabled="!canAdd">Add</Button>
        </DialogFooter>
      </form>
    </DialogContent>
  </Dialog>
</template>
