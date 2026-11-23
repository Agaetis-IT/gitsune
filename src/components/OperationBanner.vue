<script setup lang="ts">
import { TriangleAlert } from "@lucide/vue";
import { computed } from "vue";
import { Button } from "@/components/ui/button";
import { type RepoTab, useWorkspace } from "@/composables/useWorkspace";

const props = defineProps<{ tab: RepoTab }>();
const workspace = useWorkspace();

const NAMES = {
  merge: "Merge",
  rebase: "Rebase",
  cherryPick: "Cherry-pick",
  revert: "Revert",
} as const;

const operation = computed(() => props.tab.status?.operation ?? null);
const conflicts = computed(() => props.tab.status?.conflicted.length ?? 0);
/** Conflicts can also exist without an operation (a stash or a squash merge that conflicted). */
const visible = computed(() => operation.value !== null || conflicts.value > 0);

const title = computed(() => {
  const op = operation.value;
  if (!op) return "Unresolved conflicts";
  return `${NAMES[op.kind]} in progress`;
});
const detail = computed(() => {
  const parts: string[] = [];
  if (operation.value?.message) parts.push(operation.value.message);
  parts.push(
    conflicts.value > 0
      ? `${conflicts.value} file${conflicts.value === 1 ? "" : "s"} with conflicts`
      : "no conflict left",
  );
  return parts.join(" · ");
});
const busy = computed(() => props.tab.busy);
</script>

<template>
  <div
    v-if="visible"
    class="bg-destructive/10 flex flex-wrap items-center gap-2 border-b px-3 py-2 text-sm"
    role="status"
  >
    <TriangleAlert class="text-destructive size-4 shrink-0" />
    <div class="min-w-0 flex-1">
      <strong>{{ title }}</strong>
      <span class="text-muted-foreground ml-2 truncate">{{ detail }}</span>
    </div>
    <Button
      v-if="conflicts > 0"
      size="sm"
      :variant="tab.resolverPath === null ? 'default' : 'outline'"
      @click="
        tab.resolverPath === null ? workspace.openResolver(tab) : workspace.closeResolver(tab)
      "
    >
      {{ tab.resolverPath === null ? "Resolve conflicts" : "Back to the graph" }}
    </Button>
    <Button
      v-if="operation && conflicts === 0"
      size="sm"
      :disabled="busy || tab.readOnly"
      @click="workspace.continueOperation(tab)"
    >
      Continue
    </Button>
    <Button
      size="sm"
      variant="outline"
      :disabled="busy || tab.readOnly"
      :title="tab.readOnly ? 'Read-only mode is on.' : 'Cancel and go back to before it started'"
      @click="workspace.askAbortOperation(tab)"
    >
      Abort
    </Button>
  </div>
</template>
