<script setup lang="ts">
import { History, Minus, Plus, Trash2 } from "@lucide/vue";
import { computed, ref } from "vue";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Textarea } from "@/components/ui/textarea";
import { type RepoTab, useWorkspace } from "@/composables/useWorkspace";
import type { Area, FileState, StatusEntry } from "@/lib/api";

const props = defineProps<{ tab: RepoTab }>();
const workspace = useWorkspace();

/** Longest summary line before it is flagged (a convention, not a limit). */
const SUMMARY_SOFT_LIMIT = 72;

const STATE: Record<FileState, { letter: string; label: string; classes: string }> = {
  added: { letter: "A", label: "Added", classes: "text-emerald-600 dark:text-emerald-400" },
  modified: { letter: "M", label: "Modified", classes: "text-amber-600 dark:text-amber-400" },
  deleted: { letter: "D", label: "Deleted", classes: "text-rose-600 dark:text-rose-400" },
  renamed: { letter: "R", label: "Renamed", classes: "text-sky-600 dark:text-sky-400" },
  copied: { letter: "C", label: "Copied", classes: "text-sky-600 dark:text-sky-400" },
  typeChanged: {
    letter: "T",
    label: "Type changed",
    classes: "text-violet-600 dark:text-violet-400",
  },
  untracked: { letter: "U", label: "Untracked", classes: "text-muted-foreground" },
};

const sections = computed(() => [
  {
    area: "unstaged" as Area,
    title: "Unstaged",
    entries: props.tab.status?.unstaged ?? [],
    moveLabel: "Stage",
    moveAllLabel: "Stage all",
  },
  {
    area: "staged" as Area,
    title: "Staged",
    entries: props.tab.status?.staged ?? [],
    moveLabel: "Unstage",
    moveAllLabel: "Unstage all",
  },
]);

const blocked = computed(() => props.tab.readOnly || props.tab.busy);

function split(path: string): { dir: string; name: string } {
  const trimmed = path.endsWith("/") ? path.slice(0, -1) : path;
  const slash = trimmed.lastIndexOf("/");
  const name = path.slice(slash + 1);
  return slash === -1 ? { dir: "", name } : { dir: path.slice(0, slash + 1), name };
}

const isOpen = (entry: StatusEntry, area: Area) =>
  props.tab.workDiff?.entry.path === entry.path && props.tab.workDiff.area === area;

function move(area: Area, entries: StatusEntry[]) {
  const paths = entries.map((entry) => entry.path);
  if (paths.length === 0) return;
  if (area === "unstaged") void workspace.stage(props.tab, paths);
  else void workspace.unstage(props.tab, paths);
}

// ---------- discarding ----------

const discarding = ref<StatusEntry[] | null>(null);

function askDiscard(entries: StatusEntry[]) {
  if (entries.length > 0) discarding.value = entries;
}

function confirmDiscard() {
  const entries = discarding.value;
  discarding.value = null;
  if (entries)
    void workspace.discard(
      props.tab,
      entries.map((entry) => entry.path),
    );
}

// ---------- recovering discarded changes ----------

const showSnapshots = ref(false);

function toggleSnapshots() {
  showSnapshots.value = !showSnapshots.value;
  if (showSnapshots.value) void workspace.loadSnapshots(props.tab);
}

const timeFormat = new Intl.DateTimeFormat(undefined, { timeStyle: "short", dateStyle: "short" });

// ---------- commit box ----------

const summary = computed({
  get: () => props.tab.draft.summary,
  set: (value: string) => workspace.updateDraft(props.tab, { summary: value }),
});
const body = computed({
  get: () => props.tab.draft.body,
  set: (value: string) => workspace.updateDraft(props.tab, { body: value }),
});

const stagedCount = computed(() => props.tab.status?.staged.length ?? 0);
const canCommit = computed(
  () =>
    !blocked.value &&
    summary.value.trim().length > 0 &&
    (stagedCount.value > 0 || props.tab.draft.amend),
);
const commitHint = computed(() => {
  if (props.tab.readOnly) return "Read-only mode is on.";
  if (summary.value.trim() === "") return "Write a summary to commit.";
  if (stagedCount.value === 0 && !props.tab.draft.amend) return "Stage some changes to commit.";
  return "";
});

function onMessageKeydown(event: KeyboardEvent) {
  if (event.key === "Enter" && (event.metaKey || event.ctrlKey) && canCommit.value) {
    event.preventDefault();
    void workspace.commit(props.tab);
  }
}
</script>

<template>
  <aside class="flex h-full flex-col text-sm" aria-label="Working tree">
    <div class="min-h-0 flex-1 overflow-y-auto">
      <header class="border-b px-4 py-3">
        <h2 class="text-base font-semibold">Working tree</h2>
        <p class="text-muted-foreground text-xs">
          <template v-if="tab.status?.branch">on {{ tab.status.branch }}</template>
          <template v-else-if="tab.status?.detached">detached HEAD</template>
          <template v-if="tab.status?.upstream">
            · {{ tab.status.upstream }}
            <template v-if="tab.status.ahead"> ↑{{ tab.status.ahead }}</template>
            <template v-if="tab.status.behind"> ↓{{ tab.status.behind }}</template>
          </template>
        </p>
        <p
          v-if="tab.readOnly"
          class="bg-muted text-muted-foreground mt-2 rounded px-2 py-1 text-xs"
        >
          Read-only mode: nothing can be changed from here.
        </p>
      </header>

      <section v-if="tab.status?.conflicted.length" class="border-b px-4 py-3">
        <h3 class="text-destructive mb-1 text-xs font-medium">
          Conflicts ({{ tab.status.conflicted.length }})
        </h3>
        <ul class="font-mono text-xs">
          <li v-for="conflict in tab.status.conflicted" :key="conflict.path" class="truncate">
            {{ conflict.code }} {{ conflict.path }}
          </li>
        </ul>
        <p class="text-muted-foreground mt-1 text-xs">
          Resolve them in your editor or the terminal for now.
        </p>
      </section>

      <section v-for="section in sections" :key="section.area" class="border-b px-2 py-2">
        <div class="flex items-center gap-2 px-2 pb-1">
          <h3 class="text-muted-foreground text-xs font-medium">
            {{ section.title }} ({{ section.entries.length }})
          </h3>
          <div class="ml-auto flex gap-1">
            <Button
              v-if="section.area === 'unstaged' && section.entries.length"
              size="sm"
              variant="ghost"
              class="h-6 px-2 text-xs"
              :disabled="blocked"
              @click="askDiscard(section.entries)"
            >
              Discard all
            </Button>
            <Button
              v-if="section.entries.length"
              size="sm"
              variant="outline"
              class="h-6 px-2 text-xs"
              :disabled="blocked"
              @click="move(section.area, section.entries)"
            >
              {{ section.moveAllLabel }}
            </Button>
          </div>
        </div>
        <p v-if="!section.entries.length" class="text-muted-foreground px-2 py-1 text-xs">
          {{ section.area === "staged" ? "Nothing staged." : "No unstaged changes." }}
        </p>
        <ul>
          <li
            v-for="entry in section.entries"
            :key="`${section.area}:${entry.path}`"
            class="hover:bg-accent group flex items-center gap-1 rounded pr-1"
            :class="{ 'bg-accent': isOpen(entry, section.area) }"
          >
            <button
              class="flex min-w-0 flex-1 items-center gap-2 px-2 py-1 text-left"
              :title="entry.oldPath ? `${entry.oldPath} → ${entry.path}` : entry.path"
              @click="workspace.openWorkDiff(tab, entry, section.area)"
            >
              <span
                class="w-4 shrink-0 text-center text-xs font-bold"
                :class="STATE[entry.state].classes"
                :title="STATE[entry.state].label"
              >
                {{ STATE[entry.state].letter }}
              </span>
              <span class="min-w-0 flex-1 truncate text-xs">
                <span class="text-muted-foreground">{{ split(entry.path).dir }}</span>{{
                  split(entry.path).name
                }}
              </span>
            </button>
            <button
              v-if="section.area === 'unstaged'"
              class="text-muted-foreground hover:text-destructive rounded p-1 opacity-0 group-hover:opacity-100 focus-visible:opacity-100 disabled:opacity-30"
              :disabled="blocked"
              :aria-label="`Discard changes to ${entry.path}`"
              title="Discard changes"
              @click="askDiscard([entry])"
            >
              <Trash2 class="size-3.5" />
            </button>
            <button
              class="text-muted-foreground hover:text-foreground rounded p-1 opacity-0 group-hover:opacity-100 focus-visible:opacity-100 disabled:opacity-30"
              :disabled="blocked"
              :aria-label="`${section.moveLabel} ${entry.path}`"
              :title="section.moveLabel"
              @click="move(section.area, [entry])"
            >
              <component :is="section.area === 'unstaged' ? Plus : Minus" class="size-3.5" />
            </button>
          </li>
        </ul>
      </section>

      <section class="px-4 py-2">
        <button
          class="text-muted-foreground hover:text-foreground flex items-center gap-1.5 text-xs"
          :aria-expanded="showSnapshots"
          @click="toggleSnapshots"
        >
          <History class="size-3.5" />
          Recently discarded
        </button>
        <ul v-if="showSnapshots" class="mt-2 space-y-1">
          <li v-if="!tab.snapshots.length" class="text-muted-foreground text-xs">Nothing saved.</li>
          <li
            v-for="snapshot in tab.snapshots"
            :key="snapshot.reference"
            class="flex items-center gap-2 rounded border px-2 py-1 text-xs"
          >
            <span class="min-w-0 flex-1">
              <span class="block truncate" :title="snapshot.paths.join('\n')">
                {{ snapshot.paths.length }} file{{ snapshot.paths.length === 1 ? "" : "s" }} ·
                {{ snapshot.description }}
              </span>
              <span class="text-muted-foreground">
                {{ timeFormat.format(new Date(snapshot.time * 1000)) }}
              </span>
            </span>
            <Button
              size="sm"
              variant="outline"
              class="h-6 px-2 text-xs"
              :disabled="blocked"
              @click="workspace.restoreSnapshot(tab, snapshot.reference)"
            >
              Restore
            </Button>
          </li>
        </ul>
      </section>
    </div>

    <form class="space-y-2 border-t p-3" @submit.prevent="workspace.commit(tab)">
      <div class="relative">
        <input
          v-model="summary"
          class="border-input bg-background focus-visible:ring-ring/50 w-full rounded-md border px-3 py-1.5 pr-14 text-sm outline-none focus-visible:ring-2"
          placeholder="Summary"
          aria-label="Commit summary"
          :disabled="tab.busy"
          @keydown="onMessageKeydown"
        />
        <span
          class="absolute top-1/2 right-2 -translate-y-1/2 text-xs"
          :class="
            summary.length > SUMMARY_SOFT_LIMIT ? 'text-destructive' : 'text-muted-foreground'
          "
          :title="`Summaries are usually kept under ${SUMMARY_SOFT_LIMIT} characters`"
        >
          {{ summary.length }}/{{ SUMMARY_SOFT_LIMIT }}
        </span>
      </div>
      <Textarea
        v-model="body"
        class="min-h-20 text-sm"
        placeholder="Description (optional)"
        aria-label="Commit description"
        :disabled="tab.busy"
        @keydown="onMessageKeydown"
      />
      <label class="flex items-center gap-2 text-xs">
        <input
          type="checkbox"
          :checked="tab.draft.amend"
          :disabled="blocked || !tab.status?.head"
          @change="workspace.setAmend(tab, ($event.target as HTMLInputElement).checked)"
        />
        Amend the last commit
      </label>

      <div
        v-if="
          tab.writeError &&
          ['hookFailed', 'signingFailed', 'nothingToCommit'].includes(tab.writeError.kind)
        "
        class="border-destructive/50 rounded border p-2 text-xs"
        role="alert"
      >
        <p class="text-destructive font-medium">
          {{
            tab.writeError.kind === "hookFailed"
              ? "A hook rejected the commit"
              : tab.writeError.kind === "signingFailed"
                ? "The commit could not be signed"
                : "Nothing to commit"
          }}
        </p>
        <pre
          class="text-muted-foreground mt-1 max-h-40 overflow-auto font-mono whitespace-pre-wrap"
          >{{ tab.writeError.message }}</pre>
      </div>

      <Button type="submit" class="w-full" :disabled="!canCommit" :title="commitHint">
        {{
          tab.draft.amend
            ? "Amend commit"
            : `Commit${stagedCount ? ` ${stagedCount} file${stagedCount === 1 ? "" : "s"}` : ""}${tab.status?.branch ? ` to ${tab.status.branch}` : ""}`
        }}
      </Button>
      <p v-if="commitHint" class="text-muted-foreground text-center text-xs">{{ commitHint }}</p>
    </form>

    <Dialog :open="discarding !== null" @update:open="(open) => !open && (discarding = null)">
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Discard changes?</DialogTitle>
          <DialogDescription>
            The unstaged changes of
            {{ discarding?.length === 1 ? "this file" : `these ${discarding?.length} files` }}
            will be removed from the working tree. A copy is kept, so you can undo this from the
            notice that follows or from “Recently discarded”.
          </DialogDescription>
        </DialogHeader>
        <ul class="bg-muted max-h-40 overflow-auto rounded p-2 font-mono text-xs">
          <li v-for="entry in discarding" :key="entry.path" class="truncate">
            {{ entry.path }}
            <span v-if="entry.state === 'untracked'" class="text-muted-foreground">
              (untracked: the file will be deleted)
            </span>
          </li>
        </ul>
        <DialogFooter>
          <Button variant="outline" @click="discarding = null">Cancel</Button>
          <Button variant="destructive" @click="confirmDiscard">Discard</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </aside>
</template>
