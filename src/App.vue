<script setup lang="ts">
import { ref } from "vue";
import CommitPanel from "@/components/CommitPanel.vue";
import MetroGraph from "@/components/graph/MetroGraph.vue";
import RepoSidebar from "@/components/RepoSidebar.vue";
import TabBar from "@/components/TabBar.vue";
import { Button } from "@/components/ui/button";
import WelcomeScreen from "@/components/WelcomeScreen.vue";
import { useWorkspace } from "@/composables/useWorkspace";
import type { Density } from "@/lib/metro/metrics";

const workspace = useWorkspace();
const { tabs, active, activeId, density, recentRepos, openError, opening } = workspace;

const graph = ref<InstanceType<typeof MetroGraph>>();
const densities: Density[] = ["compact", "detailed"];
</script>

<template>
  <div class="bg-background text-foreground flex h-screen flex-col">
    <TabBar
      :tabs="tabs"
      :active-id="activeId"
      @activate="workspace.activate"
      @close="workspace.closeTab"
      @open="workspace.pickAndOpen"
    />

    <div v-if="active" class="flex min-h-0 flex-1">
      <div class="w-64 shrink-0 border-r">
        <RepoSidebar
          :tab="active"
          @toggle-ref="workspace.toggleRef(active, $event)"
          @toggle-kind="workspace.toggleKind(active, $event)"
          @jump="graph?.focusRow($event)"
        />
      </div>

      <main class="flex min-w-0 flex-1 flex-col">
        <div class="flex items-center gap-2 border-b px-3 py-1.5 text-xs">
          <span class="text-muted-foreground truncate" :title="active.path">
            {{ active.path }}
            <template v-if="active.headBranch"> · {{ active.headBranch }}</template>
          </span>
          <span v-if="active.loading" class="text-muted-foreground">Updating the graph…</span>
          <div class="ml-auto flex gap-1">
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
        </div>

        <p v-if="active.error" class="text-destructive border-b px-3 py-2 text-sm" role="alert">
          {{ active.error }}
        </p>

        <div class="relative min-h-0 flex-1">
          <MetroGraph
            v-if="active.source && active.source.meta.total > 0"
            :key="active.id"
            ref="graph"
            v-model:density="density"
            :selected="active.selected"
            :initial-scroll-top="active.scrollTop"
            :meta="active.source.meta"
            :fetch-page="active.source.fetchPage"
            @update:selected="workspace.select(active, $event)"
            @update:scroll-top="active.scrollTop = $event"
            @select="workspace.select(active, $event.row, $event)"
            @error="active.error = $event"
          />
          <p
            v-else
            class="text-muted-foreground absolute inset-0 flex items-center justify-center text-sm"
          >
            {{
              active.loading
                ? "Reading the history…"
                : active.source
                  ? "No commits to show."
                  : "The graph is not available."
            }}
          </p>
        </div>
      </main>

      <div class="w-80 shrink-0 border-l">
        <CommitPanel :commit="active.selectedCommit" />
      </div>
    </div>

    <WelcomeScreen
      v-else
      class="min-h-0 flex-1"
      :recents="recentRepos"
      :error="openError"
      :busy="opening"
      @open="workspace.pickAndOpen"
      @open-recent="workspace.openPath"
      @forget="workspace.forgetRecent"
    />

    <p v-if="active && openError" class="text-destructive border-t px-3 py-1 text-xs" role="alert">
      {{ openError }}
    </p>
  </div>
</template>
