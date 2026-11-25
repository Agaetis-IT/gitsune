<script setup lang="ts">
import { Check, Copy, Loader } from "@lucide/vue";
import { computed, ref, watch } from "vue";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useDiagnostics } from "@/composables/useDiagnostics";
import { useGeneralSettings } from "@/composables/useGeneralSettings";
import { useIntegrations } from "@/composables/useIntegrations";
import { type Diagnostics, getDiagnostics } from "@/lib/api";
import { buildReport, findings } from "@/lib/diagnostics";

const { open, closeDiagnostics } = useDiagnostics();
const general = useGeneralSettings();
const integrations = useIntegrations();

const data = ref<Diagnostics | null>(null);
const loading = ref(false);
const error = ref("");
const copied = ref(false);
let request = 0;

const problems = computed(() => (data.value ? findings(data.value) : []));
const report = computed(() =>
  data.value
    ? buildReport(data.value, {
        restoreTabs: general.restoreTabs.value,
        ticketPrefill: general.ticketPrefill.value,
        autoFetchMinutes: general.autoFetchMinutes.value,
        editor: integrations.editor.value,
        terminal: integrations.terminal.value,
      })
    : "",
);

/** Reads the surroundings again; a slow tool never blocks the window, it only delays the report. */
async function collect() {
  const mine = ++request;
  loading.value = true;
  error.value = "";
  try {
    const result = await getDiagnostics();
    if (mine === request) data.value = result;
  } catch (e) {
    if (mine === request) error.value = e instanceof Error ? e.message : String(e);
  } finally {
    if (mine === request) loading.value = false;
  }
}

// The report is made each time the window is opened.
watch(open, (isOpen) => {
  if (isOpen) {
    copied.value = false;
    void collect();
  }
});

async function copy() {
  try {
    await navigator.clipboard.writeText(report.value);
    copied.value = true;
    setTimeout(() => (copied.value = false), 1500);
  } catch {
    // Clipboard unavailable: the text can still be selected by hand.
  }
}
</script>

<template>
  <Dialog :open="open" @update:open="(value) => !value && closeDiagnostics()">
    <DialogContent class="max-h-[90vh] overflow-y-auto sm:max-w-3xl">
      <DialogHeader>
        <DialogTitle>Diagnostics</DialogTitle>
        <DialogDescription>
          What the application sees around it, to paste in a ticket. There is no account, e-mail,
          address, repository or branch name in it, and your home folder is written
          <code>~</code>. Nothing is sent anywhere.
        </DialogDescription>
      </DialogHeader>

      <p v-if="loading && !data" class="text-muted-foreground flex items-center gap-2 text-sm">
        <Loader class="size-4 animate-spin" /> Looking around…
      </p>
      <p v-else-if="error" class="text-destructive text-sm" role="alert">{{ error }}</p>
      <template v-else-if="data">
        <section v-if="problems.length" aria-label="To check" class="space-y-1.5">
          <h3 class="text-sm font-semibold">To check</h3>
          <ul class="list-disc space-y-1 pl-5 text-sm">
            <li v-for="problem in problems" :key="problem">{{ problem }}</li>
          </ul>
        </section>
        <p v-else class="text-sm">Nothing looks wrong.</p>
        <pre
          class="bg-muted/40 max-h-72 overflow-auto rounded border p-3 text-xs"
          aria-label="Report"
          >{{ report }}</pre>
      </template>

      <DialogFooter>
        <Button variant="outline" :disabled="loading" @click="collect">
          <Loader v-if="loading" class="size-4 animate-spin" /> Refresh
        </Button>
        <Button :disabled="!data" @click="copy">
          <component :is="copied ? Check : Copy" class="size-4" />
          {{ copied ? "Copied" : "Copy the report" }}
        </Button>
        <Button variant="outline" @click="closeDiagnostics">Close</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
