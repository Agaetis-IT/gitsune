<script setup lang="ts">
import { onMounted, ref, shallowRef } from "vue";
import MetroGraph from "@/components/graph/MetroGraph.vue";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { type GraphRow, openRepo } from "@/lib/api";
import { type GraphSource, isTauri, tauriGraphSource } from "@/lib/graphSources";
import type { Density } from "@/lib/metro/metrics";
import { type ThemeMode, useTheme } from "@/composables/useTheme";

const { mode, setMode } = useTheme();
const modes: ThemeMode[] = ["system", "light", "dark"];
const densities: Density[] = ["compact", "detailed"];

const inTauri = isTauri();
const repoPath = ref("");
const source = shallowRef<GraphSource | null>(null);
const density = ref<Density>("compact");
const selected = ref<number | null>(null);
const selectedCommit = ref<GraphRow | null>(null);
const error = ref("");
const loading = ref(false);

async function openFromPath() {
  loading.value = true;
  error.value = "";
  try {
    const info = await openRepo(repoPath.value.trim());
    source.value = await tauriGraphSource(info.id);
    selected.value = null;
    selectedCommit.value = null;
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}

// Outside Tauri (plain browser during development) show a recorded demo graph.
onMounted(async () => {
  if (!inTauri && import.meta.env.DEV) {
    const { fixtureGraphSource } = await import("@/dev/fixtureSource");
    source.value = await fixtureGraphSource();
  }
});
</script>

<template>
  <div class="bg-background text-foreground flex h-screen flex-col">
    <header class="flex flex-wrap items-center gap-3 border-b px-4 py-2">
      <h1 class="text-base font-semibold tracking-tight">git-hydra</h1>
      <form v-if="inTauri" class="flex flex-1 items-center gap-2" @submit.prevent="openFromPath">
        <Input v-model="repoPath" class="max-w-xl" placeholder="/path/to/repository" />
        <Button type="submit" :disabled="loading || !repoPath.trim()">Open</Button>
      </form>
      <div v-else class="flex-1" />
      <div class="flex gap-1">
        <Button
          v-for="d in densities"
          :key="d"
          size="sm"
          :variant="density === d ? 'default' : 'outline'"
          @click="density = d"
        >
          {{ d }}
        </Button>
      </div>
      <div class="flex gap-1">
        <Button
          v-for="m in modes"
          :key="m"
          size="sm"
          :variant="mode === m ? 'default' : 'outline'"
          @click="setMode(m)"
        >
          {{ m }}
        </Button>
      </div>
    </header>

    <p v-if="error" class="text-destructive px-4 py-2 text-sm">{{ error }}</p>

    <main class="min-h-0 flex-1">
      <MetroGraph
        v-if="source"
        v-model:selected="selected"
        v-model:density="density"
        :meta="source.meta"
        :fetch-page="source.fetchPage"
        @select="selectedCommit = $event"
        @error="error = $event"
      />
      <p v-else class="text-muted-foreground p-6 text-sm">
        {{ inTauri ? "Enter the path of a git repository." : "Loading demo graph…" }}
      </p>
    </main>

    <footer class="text-muted-foreground flex h-8 items-center border-t px-4 text-xs">
      <template v-if="selectedCommit">
        <span class="mr-3 font-mono">{{ selectedCommit.id.slice(0, 10) }}</span>
        <span class="text-foreground truncate">{{ selectedCommit.summary }}</span>
      </template>
      <template v-else-if="source">
        {{ source.meta.total }} commits · {{ source.meta.lanes.length }} lanes ·
        {{ source.meta.maxCols }} columns
      </template>
    </footer>
  </div>
</template>
