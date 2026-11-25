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

const op = computed(() => props.tab.remote);
const prompt = computed(() => op.value?.prompt ?? null);

/** Passwords and passphrases are hidden; a host key question ("yes/no") is plain text. */
const secret = computed(() => /password|passphrase|pin\b/i.test(prompt.value?.text ?? ""));

const answer = ref("");
watch(prompt, () => (answer.value = ""));

function submit() {
  void workspace.answerPrompt(props.tab, answer.value);
}
function refuse() {
  void workspace.answerPrompt(props.tab, null);
}
</script>

<template>
  <div
    v-if="op"
    class="bg-popover text-popover-foreground absolute bottom-3 left-3 z-40 w-72 rounded-md border p-3 shadow-md"
    role="status"
    aria-live="polite"
  >
    <div class="flex items-center justify-between gap-2">
      <span class="text-sm font-medium">{{ op.cancelling ? "Cancelling…" : op.label }}</span>
      <Button
        size="sm"
        variant="outline"
        :disabled="op.cancelling"
        @click="workspace.cancelRemote(tab)"
      >
        Cancel
      </Button>
    </div>
    <div class="bg-muted mt-2 h-1.5 overflow-hidden rounded" aria-hidden="true">
      <div
        class="bg-primary h-full rounded transition-[width]"
        :class="{ 'w-1/3 animate-pulse': op.percent === null }"
        :style="op.percent === null ? undefined : { width: `${op.percent}%` }"
      />
    </div>
    <p v-if="op.line" class="text-muted-foreground mt-1.5 truncate text-xs" :title="op.line">
      {{ op.line }}
    </p>
  </div>

  <Dialog :open="prompt !== null" @update:open="(value) => !value && refuse()">
    <DialogContent v-if="prompt">
      <form class="space-y-4" @submit.prevent="submit">
        <DialogHeader>
          <DialogTitle>{{ secret ? "Authentication needed" : "Confirmation needed" }}</DialogTitle>
          <DialogDescription>
            Asked by git while it talks to the remote. Nothing you type here is stored by the app.
          </DialogDescription>
        </DialogHeader>
        <label class="block text-sm">
          <span class="break-words">{{ prompt.text }}</span>
          <Input
            v-model="answer"
            :type="secret ? 'password' : 'text'"
            class="mt-2"
            autocomplete="off"
            autofocus
          />
        </label>
        <DialogFooter>
          <Button type="button" variant="outline" @click="refuse">Cancel</Button>
          <Button type="submit">Continue</Button>
        </DialogFooter>
      </form>
    </DialogContent>
  </Dialog>
</template>
