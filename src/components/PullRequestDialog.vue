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

const dialog = computed(() => {
  const current = props.tab.dialog;
  return current?.type === "createPullRequest" ? current : null;
});

const target = ref("");
const title = ref("");
const description = ref("");
const draft = ref(false);
const push = ref(true);

// Form state, filled in each time the dialog is opened (not when only its error changes).
watch(
  () => dialog.value?.branch,
  () => {
    const d = dialog.value;
    if (!d) return;
    target.value = d.target;
    title.value = d.title;
    description.value = d.description;
    draft.value = false;
    push.value = d.needsPush;
  },
  { immediate: true },
);

const busy = computed(() => props.tab.busy || props.tab.remote !== null);
const canCreate = computed(
  () =>
    !busy.value &&
    title.value.trim() !== "" &&
    target.value !== "" &&
    target.value !== dialog.value?.branch,
);

function submit() {
  if (!canCreate.value) return;
  void workspace.createPullRequest(props.tab, {
    target: target.value,
    title: title.value,
    description: description.value,
    draft: draft.value,
    push: push.value,
  });
}

const SELECT_CLASS =
  "border-input bg-background focus-visible:ring-ring/50 w-full rounded-md border px-3 py-1.5 text-sm outline-none focus-visible:ring-2";
</script>

<template>
  <Dialog :open="dialog !== null" @update:open="(open) => !open && workspace.closeDialog(tab)">
    <DialogContent v-if="dialog" class="sm:max-w-xl">
      <form class="space-y-4" @submit.prevent="submit">
        <DialogHeader>
          <DialogTitle>Create a pull request</DialogTitle>
          <DialogDescription>
            From <strong>{{ dialog.branch }}</strong> into the branch below.
          </DialogDescription>
        </DialogHeader>
        <label class="block text-sm">
          Target branch
          <select v-model="target" :class="[SELECT_CLASS, 'mt-1']" aria-label="Target branch">
            <option v-for="name in dialog.targets" :key="name" :value="name">{{ name }}</option>
          </select>
        </label>
        <label class="block text-sm">
          Title
          <Input v-model="title" class="mt-1" aria-label="Title" />
        </label>
        <label class="block text-sm">
          Description
          <Textarea v-model="description" class="mt-1 min-h-32" aria-label="Description" />
        </label>
        <label class="flex items-center gap-2 text-sm">
          <input v-model="draft" type="checkbox" /> Draft
        </label>
        <label v-if="dialog.needsPush" class="flex items-start gap-2 text-sm">
          <input v-model="push" type="checkbox" class="mt-1" />
          <span>
            Push first
            <span class="text-muted-foreground block text-xs">
              {{
                dialog.newOnRemote
                  ? `${dialog.branch} is not on ${dialog.remote} yet.`
                  : `${dialog.remote}/${dialog.branch} is at another commit.`
              }}
            </span>
          </span>
        </label>
        <p v-if="dialog.error" class="text-destructive text-sm" role="alert">
          {{ dialog.error }}
        </p>
        <DialogFooter>
          <Button type="button" variant="outline" @click="workspace.closeDialog(tab)">
            Cancel
          </Button>
          <Button type="submit" :disabled="!canCreate">
            {{ draft ? "Create the draft" : "Create" }}
          </Button>
        </DialogFooter>
      </form>
    </DialogContent>
  </Dialog>
</template>
