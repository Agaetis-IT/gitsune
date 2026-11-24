<script setup lang="ts">
import { onMounted, ref } from "vue";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { EDITORS, TERMINALS, useIntegrations } from "@/composables/useIntegrations";
import { tokenClear, tokenSet, tokenStatus } from "@/lib/api";
import { isTauri } from "@/lib/backend";

const { editor, setEditor, terminal, setTerminal } = useIntegrations();

const HOSTS = [
  {
    host: "github.com",
    label: "GitHub",
    help: "A personal access token that can read pull requests.",
  },
  {
    host: "dev.azure.com",
    label: "Azure DevOps",
    help: "A personal access token with the “Code (Read)” scope.",
  },
] as const;

/** Tokens can only be kept by the desktop application (they go to the macOS keychain). */
const available = isTauri();
const kept = ref<Record<string, boolean>>({});
const drafts = ref<Record<string, string>>({});
const message = ref("");
const failed = ref(false);

async function refresh() {
  if (!available) return;
  try {
    kept.value = Object.fromEntries(await tokenStatus());
  } catch (error) {
    message.value = String(error);
    failed.value = true;
  }
}

async function save(host: string) {
  failed.value = false;
  message.value = "";
  try {
    await tokenSet(host, drafts.value[host] ?? "");
    drafts.value[host] = "";
    message.value = "Token kept in the keychain.";
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error);
    failed.value = true;
  }
  await refresh();
}

async function forget(host: string) {
  failed.value = false;
  message.value = "";
  try {
    await tokenClear(host);
    message.value = "Token removed.";
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error);
    failed.value = true;
  }
  await refresh();
}

onMounted(refresh);
</script>

<template>
  <h2 class="text-base font-semibold">Integrations</h2>

  <fieldset class="mt-4">
    <legend class="text-sm font-medium">Editor and terminal</legend>
    <p class="text-muted-foreground mt-1 text-xs">
      Used by “Open in the editor” and “Open in the terminal” (Repository menu, and a right click on
      a file).
    </p>
    <div class="mt-2 flex flex-wrap gap-4">
      <label class="flex items-center gap-2 text-sm">
        Editor
        <select
          class="bg-background h-8 rounded-md border px-2 text-sm"
          :value="editor"
          @change="setEditor(($event.target as HTMLSelectElement).value)"
        >
          <option v-for="e in EDITORS" :key="e.id" :value="e.id">{{ e.label }}</option>
        </select>
      </label>
      <label class="flex items-center gap-2 text-sm">
        Terminal
        <select
          class="bg-background h-8 rounded-md border px-2 text-sm"
          :value="terminal"
          @change="setTerminal(($event.target as HTMLSelectElement).value)"
        >
          <option v-for="t in TERMINALS" :key="t.id" :value="t.id">{{ t.label }}</option>
        </select>
      </label>
    </div>
  </fieldset>

  <fieldset class="mt-6">
    <legend class="text-sm font-medium">Pull requests</legend>
    <p class="text-muted-foreground mt-1 text-xs">
      Without a token, “Open or create a pull request” opens the page that starts one. With a token
      for the host, the application first looks for an open pull request of the branch and opens it.
      Tokens are kept in the macOS keychain and are never shown again.
    </p>
    <p v-if="!available" class="text-muted-foreground mt-2 text-sm">
      Tokens can only be kept by the desktop application.
    </p>
    <ul v-else class="mt-3 space-y-3">
      <li v-for="h in HOSTS" :key="h.host">
        <div class="flex items-center gap-2 text-sm">
          <span class="w-28 font-medium">{{ h.label }}</span>
          <span class="text-muted-foreground text-xs">
            {{ kept[h.host] ? "A token is kept" : "No token" }}
          </span>
        </div>
        <p class="text-muted-foreground text-xs">{{ h.help }}</p>
        <form class="mt-1 flex gap-2" @submit.prevent="save(h.host)">
          <Input
            v-model="drafts[h.host]"
            type="password"
            autocomplete="off"
            spellcheck="false"
            class="h-8 max-w-sm"
            :aria-label="`Token for ${h.label}`"
            placeholder="Paste a token"
          />
          <Button size="sm" type="submit" :disabled="!(drafts[h.host] ?? '').trim()">Save</Button>
          <Button
            size="sm"
            type="button"
            variant="outline"
            :disabled="!kept[h.host]"
            @click="forget(h.host)"
          >
            Remove
          </Button>
        </form>
      </li>
    </ul>
    <p
      v-if="message"
      class="mt-2 text-sm"
      :class="failed ? 'text-destructive' : 'text-muted-foreground'"
      :role="failed ? 'alert' : 'status'"
    >
      {{ message }}
    </p>
  </fieldset>
</template>
