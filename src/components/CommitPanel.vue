<script setup lang="ts">
import { Check, Copy, Loader } from "@lucide/vue";
import { computed, ref, watch } from "vue";
import { useFileMenu } from "@/composables/useFileMenu";
import type { ChangeStatus, CommitDetail, FileChange, GraphRow } from "@/lib/api";
import type { HostedRepo } from "@/lib/remoteUrl";
import { splitTickets } from "@/lib/tickets";

const props = defineProps<{
  commit: GraphRow | null;
  detail: CommitDetail | null;
  loading: boolean;
  error: string;
  /** Path of the file whose diff is open. */
  openPath: string | null;
  /** The hosting service of the repository; without it, ticket references stay plain text. */
  hosted: HostedRepo | null;
}>();
const emit = defineEmits<{ openFile: [file: FileChange]; openTicket: [ticket: string] }>();

const { openFileMenu } = useFileMenu();
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

const titleParts = computed(() => splitTickets(props.commit?.summary ?? ""));
const bodyParts = computed(() => splitTickets(body.value));

const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "full", timeStyle: "short" });

/** Message without its first line (shown as the title). */
const body = computed(() => {
  const message = props.detail?.message ?? "";
  const newline = message.indexOf("\n");
  return newline === -1 ? "" : message.slice(newline + 1).trim();
});

const STATUS: Record<ChangeStatus, { letter: string; label: string; classes: string }> = {
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
};

function split(path: string): { dir: string; name: string } {
  const slash = path.lastIndexOf("/");
  return slash === -1
    ? { dir: "", name: path }
    : { dir: path.slice(0, slash + 1), name: path.slice(slash + 1) };
}
</script>

<template>
  <aside class="h-full overflow-y-auto p-4 text-sm" aria-label="Commit details">
    <div v-if="!commit" class="text-muted-foreground space-y-2">
      <p>Select a commit to see its details.</p>
      <p class="text-xs">
        Double-click the last commit of a branch, or its label, to switch to that branch.
      </p>
    </div>
    <template v-else>
      <h2 class="text-base leading-snug font-semibold break-words">
        <template v-for="(part, index) in titleParts" :key="index">
          <button
            v-if="part.ticket && hosted"
            class="text-primary underline-offset-2 hover:underline"
            :title="`Open ticket ${part.ticket}`"
            @click="emit('openTicket', part.ticket)"
          >
            {{ part.text }}
          </button>
          <template v-else>{{ part.text }}</template>
        </template>
      </h2>
      <p v-if="body" class="text-muted-foreground mt-2 text-xs break-words whitespace-pre-wrap">
        <template v-for="(part, index) in bodyParts" :key="index">
          <button
            v-if="part.ticket && hosted"
            class="text-primary underline-offset-2 hover:underline"
            :title="`Open ticket ${part.ticket}`"
            @click="emit('openTicket', part.ticket)"
          >
            {{ part.text }}
          </button>
          <template v-else>{{ part.text }}</template>
        </template>
      </p>

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

      <section class="mt-5 border-t pt-3" aria-label="Changed files">
        <h3 class="text-muted-foreground mb-2 flex items-center gap-2 text-xs font-medium">
          Changed files
          <span v-if="detail"
            >({{ detail.files.length }}{{ detail.filesTruncated ? "+" : "" }})</span
          >
          <Loader v-if="loading" class="size-3.5 animate-spin" />
        </h3>
        <p v-if="error" class="text-destructive text-xs" role="alert">{{ error }}</p>
        <p v-else-if="detail && !detail.files.length" class="text-muted-foreground text-xs">
          No file changes (empty commit).
        </p>
        <ul v-if="detail" class="-mx-1">
          <li v-for="file in detail.files" :key="file.path">
            <button
              class="hover:bg-accent flex w-full items-center gap-2 rounded px-1 py-1 text-left"
              :class="{ 'bg-accent': file.path === openPath }"
              :title="file.oldPath ? `${file.oldPath} → ${file.path}` : file.path"
              @click="emit('openFile', file)"
              @contextmenu="openFileMenu(file.path, $event)"
            >
              <span
                class="w-4 shrink-0 text-center text-xs font-bold"
                :class="STATUS[file.status].classes"
                :title="STATUS[file.status].label"
              >
                {{ STATUS[file.status].letter }}
              </span>
              <span class="min-w-0 flex-1 truncate text-xs">
                <span class="text-muted-foreground">{{ split(file.path).dir }}</span>{{
                  split(file.path).name
                }}
              </span>
              <span v-if="file.binary" class="text-muted-foreground shrink-0 text-[10px]">bin</span>
              <span
                v-else-if="file.additions !== null"
                class="shrink-0 font-mono text-[10px] leading-none"
              >
                <span class="text-emerald-600 dark:text-emerald-400">+{{ file.additions }}</span>
                <span class="text-rose-600 dark:text-rose-400"> −{{ file.deletions }}</span>
              </span>
            </button>
          </li>
        </ul>
      </section>
    </template>
  </aside>
</template>
