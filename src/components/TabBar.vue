<script setup lang="ts">
import { Plus, X } from "@lucide/vue";
import type { RepoTab } from "@/composables/useWorkspace";

defineProps<{ tabs: readonly RepoTab[]; activeId: string | null }>();
const emit = defineEmits<{ activate: [id: string]; close: [id: string]; open: [] }>();
</script>

<template>
  <div class="bg-muted/40 flex items-stretch border-b">
    <div class="flex min-w-0 flex-1 items-stretch overflow-x-auto" role="tablist">
      <button
        v-for="tab in tabs"
        :key="tab.id"
        role="tab"
        :aria-selected="tab.id === activeId"
        :title="tab.path"
        class="group flex max-w-52 min-w-28 items-center gap-2 border-r px-3 text-sm"
        :class="
          tab.id === activeId
            ? 'bg-background text-foreground'
            : 'text-muted-foreground hover:bg-background/60'
        "
        @click="emit('activate', tab.id)"
        @auxclick.middle.prevent="emit('close', tab.id)"
      >
        <span class="truncate">{{ tab.name }}</span>
        <span
          role="button"
          :aria-label="`Close ${tab.name}`"
          class="hover:bg-accent ml-auto rounded p-0.5 opacity-60 group-hover:opacity-100"
          @click.stop="emit('close', tab.id)"
        >
          <X class="size-3.5" />
        </span>
      </button>
      <button
        class="text-muted-foreground hover:bg-background/60 hover:text-foreground px-3"
        aria-label="Open a repository"
        title="Open a repository"
        @click="emit('open')"
      >
        <Plus class="size-4" />
      </button>
    </div>
  </div>
</template>
