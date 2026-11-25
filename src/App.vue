<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import CommitPanel from "@/components/CommitPanel.vue";
import DiffView from "@/components/DiffView.vue";
import LanePopover from "@/components/LanePopover.vue";
import MetroGraph from "@/components/graph/MetroGraph.vue";
import RepoSidebar from "@/components/RepoSidebar.vue";
import SearchBar from "@/components/SearchBar.vue";
import TabBar from "@/components/TabBar.vue";
import { Button } from "@/components/ui/button";
import WelcomeScreen from "@/components/WelcomeScreen.vue";
import { useWorkspace } from "@/composables/useWorkspace";
import type { Lane } from "@/lib/api";
import { refsOfBranch } from "@/lib/branchRefs";
import type { Density } from "@/lib/metro/metrics";

const workspace = useWorkspace();
const { tabs, active, activeId, density, diffMode, recentRepos, openError, opening } = workspace;

const graph = ref<InstanceType<typeof MetroGraph>>();
const searchBar = ref<InstanceType<typeof SearchBar>>();
const lanePopover = ref<{ lane: Lane; x: number; y: number } | null>(null);
const densities: Density[] = ["compact", "detailed"];

/** Name of the highlighted lane, for the chip in the toolbar. */
const highlightName = computed(() => {
  const tab = active.value;
  if (!tab || tab.highlightLane === null) return "";
  return tab.source?.meta.lanes[tab.highlightLane]?.name ?? "this line";
});

async function runSearch(query: string) {
  const tab = active.value;
  if (!tab) return;
  await workspace.search(tab, query);
  jumpToMatch(workspace.active.value?.search.rows[workspace.active.value.search.current]);
}

function stepMatch(delta: 1 | -1) {
  const tab = active.value;
  if (tab) jumpToMatch(workspace.stepMatch(tab, delta));
}

function jumpToMatch(row: number | null | undefined) {
  if (row !== null && row !== undefined) graph.value?.focusRow(row);
}

function highlightFromPopover() {
  const tab = active.value;
  if (tab && lanePopover.value) workspace.toggleHighlight(tab, lanePopover.value.lane.id);
  lanePopover.value = null;
}

function hideFromPopover() {
  const tab = active.value;
  const name = lanePopover.value?.lane.name;
  lanePopover.value = null;
  if (tab && name) void workspace.hideBranch(tab, name);
}

const canHideLane = computed(() => {
  const tab = active.value;
  const name = lanePopover.value?.lane.name;
  return !!tab && !!name && refsOfBranch(tab.refs, name).length > 0;
});

// Ctrl/Cmd+F goes to the search box.
function onKeydown(event: KeyboardEvent) {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f" && active.value) {
    event.preventDefault();
    searchBar.value?.focus();
  }
}
onMounted(() => document.addEventListener("keydown", onKeydown));
onBeforeUnmount(() => document.removeEventListener("keydown", onKeydown));
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
          @toggle-highlight="workspace.toggleHighlight(active, $event)"
          @jump="graph?.focusRow($event)"
        />
      </div>

      <main class="flex min-w-0 flex-1 flex-col">
        <div class="flex items-center gap-2 border-b px-3 py-1.5 text-xs">
          <span class="text-muted-foreground min-w-0 truncate" :title="active.path">
            {{ active.path }}
            <template v-if="active.headBranch"> · {{ active.headBranch }}</template>
          </span>
          <span v-if="active.loading" class="text-muted-foreground">Updating the graph…</span>
          <button
            v-if="active.highlightLane !== null"
            class="bg-accent hover:bg-accent/70 flex shrink-0 items-center gap-1 rounded px-2 py-0.5 whitespace-nowrap"
            title="Remove the highlight"
            @click="workspace.clearHighlight(active)"
          >
            Highlighting {{ highlightName }} ✕
          </button>
          <div class="ml-auto flex items-center gap-2">
            <SearchBar
              :key="active.id"
              ref="searchBar"
              :query="active.search.query"
              :total="active.search.total"
              :current="active.search.current"
              :loading="active.search.loading"
              :error="active.search.error"
              @search="runSearch"
              @step="stepMatch"
              @clear="workspace.clearSearch(active)"
            />
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
            :anchor="active.anchor"
            :meta="active.source.meta"
            :fetch-page="active.source.fetchPage"
            :highlight-lane="active.highlightLane"
            :matches="active.search.rows"
            @lane-click="lanePopover = $event"
            @update:selected="workspace.select(active, $event)"
            @update:scroll-top="active.scrollTop = $event"
            @update:top-commit="active.topAnchor = $event"
            @select="workspace.select(active, $event.row, $event)"
            @error="active.error = $event"
          />
          <LanePopover
            v-if="lanePopover && active.source"
            :lane="lanePopover.lane"
            :x="lanePopover.x"
            :y="lanePopover.y"
            :highlighted="active.highlightLane === lanePopover.lane.id"
            :can-hide="canHideLane"
            @highlight="highlightFromPopover"
            @hide="hideFromPopover"
            @close="lanePopover = null"
          />
          <p
            v-else-if="!active.source || active.source.meta.total === 0"
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
          <DiffView
            v-if="active.openFile && active.selectedCommit"
            :key="`${active.selectedCommit.id}:${active.openFile.path}`"
            v-model:mode="diffMode"
            class="absolute inset-0 z-20"
            :file="active.openFile"
            :commit-id="active.selectedCommit.id"
            :load="(force) => workspace.fileDiff(active!, force)"
            @close="workspace.openFile(active, null)"
          />
        </div>
      </main>

      <div class="w-80 shrink-0 border-l">
        <CommitPanel
          :commit="active.selectedCommit"
          :detail="active.detail"
          :loading="active.detailLoading"
          :error="active.detailError"
          :open-path="active.openFile?.path ?? null"
          @open-file="workspace.openFile(active, $event)"
        />
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
