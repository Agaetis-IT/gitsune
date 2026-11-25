<script setup lang="ts">
import { Check, Copy } from "@lucide/vue";
import { ref, watch } from "vue";
import type { GraphRow } from "@/lib/api";

const props = defineProps<{ commit: GraphRow | null }>();

const copied = ref(false);
let resetTimer: ReturnType<typeof setTimeout> | undefined;

watch(
  () => props.commit?.id,
  () => {
    copied.value = false;
  },
);

async function copyId() {
  if (!props.commit) return;
  try {
    await navigator.clipboard.writeText(props.commit.id);
    copied.value = true;
    clearTimeout(resetTimer);
    resetTimer = setTimeout(() => (copied.value = false), 1500);
  } catch {
    // Clipboard unavailable: nothing to do.
  }
}

const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "full", timeStyle: "short" });
</script>

<template>
  <aside class="h-full overflow-y-auto p-4 text-sm" aria-label="Commit details">
    <p v-if="!commit" class="text-muted-foreground">Select a commit to see its details.</p>
    <template v-else>
      <h2 class="text-base leading-snug font-semibold break-words">{{ commit.summary }}</h2>

      <dl class="mt-4 space-y-3">
        <div>
          <dt class="text-muted-foreground text-xs">Commit</dt>
          <dd class="flex items-start gap-1">
            <code class="text-xs break-all">{{ commit.id }}</code>
            <button
              class="text-muted-foreground hover:text-foreground shrink-0 rounded p-0.5"
              aria-label="Copy the commit id"
              title="Copy the commit id"
              @click="copyId"
            >
              <component :is="copied ? Check : Copy" class="size-3.5" />
            </button>
          </dd>
        </div>
        <div>
          <dt class="text-muted-foreground text-xs">Author</dt>
          <dd>{{ commit.author || "Unknown" }}</dd>
        </div>
        <div>
          <dt class="text-muted-foreground text-xs">Date</dt>
          <dd>{{ dateFormat.format(new Date(commit.time * 1000)) }}</dd>
        </div>
        <div>
          <dt class="text-muted-foreground text-xs">
            {{ commit.parents.length > 1 ? "Parents (merge)" : "Parent" }}
          </dt>
          <dd v-if="commit.parents.length" class="font-mono text-xs">
            <div v-for="parent in commit.parents" :key="parent">{{ parent.slice(0, 10) }}</div>
          </dd>
          <dd v-else class="text-muted-foreground">None (root commit)</dd>
        </div>
        <div v-if="commit.refs.length">
          <dt class="text-muted-foreground mb-1 text-xs">Refs</dt>
          <dd class="flex flex-wrap gap-1">
            <span
              v-for="reference in commit.refs"
              :key="reference.fullName"
              class="rounded border px-1.5 py-0.5 text-xs"
              :class="{ 'bg-primary text-primary-foreground border-transparent': reference.isHead }"
            >
              {{ reference.name }}
            </span>
          </dd>
        </div>
      </dl>
    </template>
  </aside>
</template>
