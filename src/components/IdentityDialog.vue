<script setup lang="ts">
import { ref, watch } from "vue";
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

const name = ref("");
const email = ref("");
const global = ref(true);

// Start from what git already knows, when the dialog opens.
watch(
  () => props.tab.identityRequired,
  () => {
    if (props.tab.identityRequired) {
      name.value = "";
      email.value = "";
    }
  },
);

const complete = () => name.value.trim() !== "" && email.value.trim() !== "";

function submit() {
  if (!complete()) return;
  void workspace.submitIdentity(props.tab, name.value, email.value, global.value);
}
</script>

<template>
  <Dialog
    :open="tab.identityRequired"
    @update:open="(open) => !open && workspace.cancelIdentity(tab)"
  >
    <DialogContent>
      <form class="space-y-4" @submit.prevent="submit">
        <DialogHeader>
          <DialogTitle>Who are you?</DialogTitle>
          <DialogDescription>
            git needs a name and an email address to sign your commits with. They are stored in
            git's configuration, never sent anywhere by this app.
          </DialogDescription>
        </DialogHeader>
        <label class="block text-sm">
          Name
          <Input v-model="name" class="mt-1" autocomplete="name" placeholder="Your name" />
        </label>
        <label class="block text-sm">
          Email
          <Input v-model="email" class="mt-1" type="email" placeholder="you@example.com" />
        </label>
        <fieldset class="space-y-1 text-sm">
          <label class="flex items-center gap-2">
            <input v-model="global" type="radio" :value="true" /> For every repository
          </label>
          <label class="flex items-center gap-2">
            <input v-model="global" type="radio" :value="false" /> Only for this repository
          </label>
        </fieldset>
        <DialogFooter>
          <Button type="button" variant="outline" @click="workspace.cancelIdentity(tab)">
            Cancel
          </Button>
          <Button type="submit" :disabled="!complete() || tab.busy">Save and commit</Button>
        </DialogFooter>
      </form>
    </DialogContent>
  </Dialog>
</template>
