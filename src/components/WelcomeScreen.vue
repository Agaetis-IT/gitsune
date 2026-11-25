<script setup lang="ts">
import { FolderOpen, X } from "@lucide/vue";
import { Button } from "@/components/ui/button";
import type { RecentRepo } from "@/lib/recents";

defineProps<{ recents: readonly RecentRepo[]; error: string; busy: boolean }>();
const emit = defineEmits<{ open: []; openRecent: [path: string]; forget: [path: string] }>();
</script>

<template>
  <div class="flex h-full items-center justify-center p-8">
    <div class="w-full max-w-md">
      <h1 class="text-2xl font-semibold tracking-tight">Gitsune</h1>
      <p class="text-muted-foreground mt-1 text-sm">Browse the history of a git repository.</p>

      <Button class="mt-6" :disabled="busy" @click="emit('open')">
        <FolderOpen class="size-4" />
        Open a repository
      </Button>
      <p v-if="error" class="text-destructive mt-3 text-sm break-words" role="alert">{{ error }}</p>

      <section v-if="recents.length" class="mt-8">
        <h2 class="text-muted-foreground mb-2 text-xs font-medium tracking-wide uppercase">
          Recent
        </h2>
        <ul class="divide-y rounded-md border">
          <li
            v-for="repo in recents"
            :key="repo.path"
            class="hover:bg-accent group flex items-center"
          >
            <button
              class="min-w-0 flex-1 px-3 py-2 text-left"
              :disabled="busy"
              @click="emit('openRecent', repo.path)"
            >
              <span class="block truncate text-sm font-medium">{{ repo.name }}</span>
              <span class="text-muted-foreground block truncate text-xs">{{ repo.path }}</span>
            </button>
            <button
              class="text-muted-foreground hover:text-foreground mr-2 rounded p-1 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
              :aria-label="`Remove ${repo.name} from recent repositories`"
              title="Remove from recent"
              @click="emit('forget', repo.path)"
            >
              <X class="size-3.5" />
            </button>
          </li>
        </ul>
      </section>
    </div>
  </div>
</template>
