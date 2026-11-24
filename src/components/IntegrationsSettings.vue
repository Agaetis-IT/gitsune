<script setup lang="ts">
import { onMounted, ref } from "vue";
import { Button } from "@/components/ui/button";
import { EDITORS, TERMINALS, useIntegrations } from "@/composables/useIntegrations";
import { azureSignIn, type HostStatus, hostStatuses } from "@/lib/api";
import { isTauri } from "@/lib/backend";

const { editor, setEditor, terminal, setTerminal } = useIntegrations();

const HOSTS = [
  {
    host: "github.com",
    label: "GitHub",
    tool: "GitHub CLI",
    signIn: "gh auth login",
    install: "brew install gh",
  },
  {
    host: "dev.azure.com",
    label: "Azure DevOps",
    tool: "Azure CLI",
    signIn: "az login",
    install: "brew install azure-cli",
  },
  {
    host: "gitlab.com",
    label: "GitLab",
    tool: "GitLab CLI",
    signIn: "glab auth login",
    install: "brew install glab",
  },
] as const;

/** The sign-in is the one of the command line tools: only the desktop application runs them. */
const available = isTauri();
const statuses = ref<Record<string, HostStatus>>({});
const loading = ref(false);
const signingIn = ref(false);
const message = ref("");
const failed = ref(false);
const copied = ref("");

async function refresh() {
  if (!available) return;
  loading.value = true;
  try {
    statuses.value = Object.fromEntries((await hostStatuses()).map((s) => [s.host, s]));
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error);
    failed.value = true;
  } finally {
    loading.value = false;
  }
}

async function signInAzure() {
  signingIn.value = true;
  failed.value = false;
  message.value = "Finish the sign-in in your browser…";
  try {
    await azureSignIn();
    message.value = "Signed in with the Azure CLI.";
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error);
    failed.value = true;
  } finally {
    signingIn.value = false;
  }
  await refresh();
}

async function copy(command: string) {
  try {
    await navigator.clipboard.writeText(command);
    copied.value = command;
  } catch {
    // Clipboard unavailable: the command is shown anyway.
  }
}

function stateText(status: HostStatus | undefined, tool: string): string {
  if (!status) return loading.value ? "Checking…" : "Unknown";
  if (status.state === "signedIn") {
    return `Signed in with the ${status.tool}${status.account ? ` (${status.account})` : ""}`;
  }
  return status.state === "missing" ? `The ${tool} is not installed` : "Not signed in";
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
      “Open or create a pull request” opens the page that starts one. When you are signed in with
      the Azure CLI, the GitHub CLI or the GitLab CLI, the application first looks for an open pull
      request (a merge request on GitLab) of the branch and opens it. GitLab instances of your own
      use the same sign-in, with <code>glab auth login --hostname your.host</code>. It uses your
      sign-in without keeping any password or token.
    </p>
    <p v-if="!available" class="text-muted-foreground mt-2 text-sm">
      Sign-in is only available in the desktop application.
    </p>
    <ul v-else class="mt-3 space-y-3">
      <li v-for="h in HOSTS" :key="h.host">
        <div class="flex flex-wrap items-center gap-2 text-sm">
          <span class="w-28 font-medium">{{ h.label }}</span>
          <span class="text-muted-foreground text-xs">
            {{ stateText(statuses[h.host], h.tool) }}
          </span>
        </div>
        <div
          v-if="statuses[h.host] && statuses[h.host].state !== 'signedIn'"
          class="mt-1 flex flex-wrap items-center gap-2"
        >
          <Button
            v-if="h.host === 'dev.azure.com' && statuses[h.host].state === 'signedOut'"
            size="sm"
            :disabled="signingIn"
            @click="signInAzure"
          >
            {{ signingIn ? "Waiting for the browser…" : "Sign in with the Azure CLI" }}
          </Button>
          <template v-else>
            <code class="bg-muted rounded px-1.5 py-0.5 text-xs">
              {{ statuses[h.host].state === "missing" ? h.install : h.signIn }}
            </code>
            <Button
              size="sm"
              variant="outline"
              @click="copy(statuses[h.host].state === 'missing' ? h.install : h.signIn)"
            >
              {{
                copied === (statuses[h.host].state === "missing" ? h.install : h.signIn)
                  ? "Copied"
                  : "Copy the command"
              }}
            </Button>
            <span class="text-muted-foreground text-xs">then run it in a terminal</span>
          </template>
        </div>
      </li>
    </ul>
    <div v-if="available" class="mt-2">
      <Button size="sm" variant="ghost" :disabled="loading" @click="refresh">Check again</Button>
    </div>
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
