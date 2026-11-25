<script setup lang="ts">
import { X } from "@lucide/vue";
import { Button } from "@/components/ui/button";
import { useWorkspace } from "@/composables/useWorkspace";

const workspace = useWorkspace();
const { notices } = workspace;

async function run(id: number, action: () => void | Promise<void>) {
  workspace.dismissNotice(id);
  await action();
}
</script>

<template>
  <div
    class="pointer-events-none fixed right-4 bottom-12 z-50 flex w-96 flex-col items-end gap-2"
    aria-live="polite"
  >
    <div
      v-for="notice in notices"
      :key="notice.id"
      class="bg-popover text-popover-foreground pointer-events-auto flex w-full items-start gap-2 rounded-md border p-3 text-sm shadow-md"
      :class="{ 'border-destructive/60': notice.kind === 'error' }"
      :role="notice.kind === 'error' ? 'alert' : 'status'"
    >
      <p
        class="min-w-0 flex-1 break-words"
        :class="{ 'text-destructive': notice.kind === 'error' }"
      >
        {{ notice.text }}
      </p>
      <Button
        v-if="notice.action"
        size="sm"
        variant="outline"
        class="h-6 px-2 text-xs"
        @click="run(notice.id, notice.action.run)"
      >
        {{ notice.action.label }}
      </Button>
      <button
        class="text-muted-foreground hover:text-foreground rounded p-0.5"
        aria-label="Dismiss"
        @click="workspace.dismissNotice(notice.id)"
      >
        <X class="size-3.5" />
      </button>
    </div>
  </div>
</template>
