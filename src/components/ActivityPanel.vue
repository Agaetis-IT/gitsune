<script setup lang="ts">
import { Check, Copy, X } from "@lucide/vue";
import { computed, ref } from "vue";
import { Button } from "@/components/ui/button";
import { type RepoTab, useWorkspace } from "@/composables/useWorkspace";

const props = defineProps<{ tab: RepoTab }>();
const emit = defineEmits<{ close: [] }>();
const workspace = useWorkspace();

/** Newest first. */
const entries = computed(() => [...props.tab.activity].reverse());
const expanded = ref<Set<number>>(new Set());
const copied = ref<number | null>(null);

const timeFormat = new Intl.DateTimeFormat(undefined, { timeStyle: "medium" });

function toggle(id: number) {
  const next = new Set(expanded.value);
  if (!next.delete(id)) next.add(id);
  expanded.value = next;
}

async function copy(id: number, command: string) {
  try {
    await navigator.clipboard.writeText(command);
    copied.value = id;
    setTimeout(() => (copied.value = null), 1500);
  } catch {
    // Clipboard unavailable.
  }
}
</script>

<template>
  <aside
    class="bg-background absolute top-0 right-0 z-40 flex h-full w-[28rem] max-w-full flex-col border-l shadow-lg"
    aria-label="Activity log"
  >
    <header class="flex items-center gap-2 border-b px-3 py-2">
      <h2 class="text-sm font-semibold">Activity</h2>
      <span class="text-muted-foreground text-xs">git commands run by the app</span>
      <Button
        size="sm"
        variant="ghost"
        class="ml-auto h-6 px-2 text-xs"
        :disabled="!entries.length"
        @click="workspace.clearActivity(tab)"
      >
        Clear
      </Button>
      <button
        class="text-muted-foreground hover:text-foreground rounded p-1"
        aria-label="Close the activity log"
        @click="emit('close')"
      >
        <X class="size-4" />
      </button>
    </header>

    <p v-if="!entries.length" class="text-muted-foreground p-4 text-sm">
      Nothing yet. Staging, committing and other changes made from the app are listed here, with the
      exact command and its output. Reading the status in the background is not logged.
    </p>
    <ul class="min-h-0 flex-1 overflow-y-auto">
      <li v-for="entry in entries" :key="entry.id" class="border-b">
        <div class="flex items-start gap-2 px-3 py-2">
          <span
            class="mt-1.5 size-2 shrink-0 rounded-full"
            :class="entry.ok ? 'bg-emerald-500' : 'bg-rose-500'"
            :title="entry.ok ? 'succeeded' : `failed (exit code ${entry.exitCode})`"
          />
          <button class="min-w-0 flex-1 text-left" @click="toggle(entry.id)">
            <code class="block font-mono text-xs break-all">{{ entry.command }}</code>
            <span class="text-muted-foreground text-xs">
              {{ timeFormat.format(new Date(entry.startedAt)) }} · {{ entry.durationMs }} ms
            </span>
          </button>
          <button
            class="text-muted-foreground hover:text-foreground shrink-0 rounded p-1"
            aria-label="Copy the command"
            title="Copy the command"
            @click="copy(entry.id, entry.command)"
          >
            <component :is="copied === entry.id ? Check : Copy" class="size-3.5" />
          </button>
        </div>
        <div v-if="expanded.has(entry.id)" class="bg-muted/50 space-y-2 px-3 py-2">
          <pre
            v-if="entry.stdout"
            class="max-h-48 overflow-auto font-mono text-xs whitespace-pre-wrap"
            >{{ entry.stdout }}</pre>
          <pre
            v-if="entry.stderr"
            class="text-muted-foreground max-h-48 overflow-auto font-mono text-xs whitespace-pre-wrap"
            >{{ entry.stderr }}</pre>
          <p v-if="!entry.stdout && !entry.stderr" class="text-muted-foreground text-xs">
            No output.
          </p>
          <p v-if="entry.truncated" class="text-muted-foreground text-xs">Output cut at 16 KiB.</p>
        </div>
      </li>
    </ul>
  </aside>
</template>
