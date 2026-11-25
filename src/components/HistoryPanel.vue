<script setup lang="ts">
import { X } from "@lucide/vue";
import { computed } from "vue";
import { Button } from "@/components/ui/button";
import { type RepoTab, useWorkspace } from "@/composables/useWorkspace";

const props = defineProps<{ tab: RepoTab }>();
const emit = defineEmits<{ close: [] }>();
const workspace = useWorkspace();

const entries = computed(() => props.tab.reflog);
const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "short", timeStyle: "short" });

/** `commit: Add a file` is shown as the kind (`commit`) and the detail (`Add a file`). */
function split(message: string): { kind: string; detail: string } {
  const colon = message.indexOf(":");
  return colon < 0
    ? { kind: message, detail: "" }
    : { kind: message.slice(0, colon), detail: message.slice(colon + 1).trim() };
}
</script>

<template>
  <aside
    class="bg-background absolute top-0 right-0 z-40 flex h-full w-[28rem] max-w-full flex-col border-l shadow-lg"
    aria-label="History of positions"
  >
    <header class="flex items-center gap-2 border-b px-3 py-2">
      <h2 class="text-sm font-semibold">History</h2>
      <span class="text-muted-foreground text-xs">where HEAD has been</span>
      <Button
        size="sm"
        variant="outline"
        class="ml-auto h-6 px-2 text-xs"
        :disabled="tab.busy || tab.readOnly || entries.length < 2"
        title="Take back the last operation that moved HEAD"
        @click="workspace.undoLast(tab)"
      >
        Undo last
      </Button>
      <button
        class="text-muted-foreground hover:text-foreground rounded p-1"
        aria-label="Close the history"
        @click="emit('close')"
      >
        <X class="size-4" />
      </button>
    </header>

    <p v-if="!entries.length" class="text-muted-foreground p-4 text-sm">
      Nothing yet. Every commit, checkout, merge, rebase or reset moves HEAD and is listed here, so
      you can go back to any earlier position.
    </p>
    <ul class="min-h-0 flex-1 overflow-y-auto">
      <li
        v-for="(entry, index) in entries"
        :key="`${index}-${entry.id}`"
        class="flex items-start gap-2 border-b px-3 py-2"
      >
        <div class="min-w-0 flex-1">
          <p class="text-xs break-words">
            <span class="font-semibold">{{ split(entry.message).kind }}</span>
            <span v-if="split(entry.message).detail" class="text-muted-foreground">
              {{ split(entry.message).detail }}
            </span>
          </p>
          <p class="text-muted-foreground text-xs">
            <code class="font-mono">{{ entry.id.slice(0, 7) }}</code>
            · {{ dateFormat.format(new Date(entry.time * 1000)) }}
          </p>
        </div>
        <span v-if="entry.current" class="text-muted-foreground shrink-0 text-xs">now</span>
        <Button
          v-else
          size="sm"
          variant="ghost"
          class="h-6 shrink-0 px-2 text-xs"
          :disabled="tab.busy || tab.readOnly"
          title="Move the branch back here (refused if it would overwrite your changes)"
          @click="workspace.restoreHead(tab, entry)"
        >
          Restore here
        </Button>
      </li>
    </ul>
  </aside>
</template>
