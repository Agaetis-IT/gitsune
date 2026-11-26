<script setup lang="ts">
import { Loader } from "@lucide/vue";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useUpdates } from "@/composables/useUpdates";
import { openExternal } from "@/lib/api";

const { open, checking, result, error, close, checkNow } = useUpdates();

async function download() {
  if (!result.value) return;
  try {
    await openExternal(result.value.url);
  } catch {
    // The browser could not be started: the address is shown in the window.
  }
}
</script>

<template>
  <Dialog :open="open" @update:open="(value) => !value && close()">
    <DialogContent class="max-h-[90vh] overflow-y-auto sm:max-w-lg">
      <DialogHeader>
        <DialogTitle>
          {{ result?.available ? "A new version is available" : "Updates" }}
        </DialogTitle>
        <DialogDescription v-if="result?.available">
          Gitsune {{ result.latest }} is out; you have {{ result.current }}. The packages are on the
          page of the release: download the one for your system and install it over the current one.
        </DialogDescription>
        <DialogDescription v-else>
          The latest published release is read from GitHub. Nothing about you or your repositories
          is sent.
        </DialogDescription>
      </DialogHeader>

      <p v-if="checking" class="text-muted-foreground flex items-center gap-2 text-sm">
        <Loader class="size-4 animate-spin" /> Looking for a new version…
      </p>
      <p v-else-if="error" class="text-destructive text-sm" role="alert">{{ error }}</p>
      <p v-else-if="result && !result.available" class="text-sm">
        Gitsune {{ result.current }} is the latest version.
      </p>
      <template v-else-if="result">
        <h3 class="text-sm font-semibold">What is new in {{ result.latest }}</h3>
        <pre
          v-if="result.notes"
          class="bg-muted max-h-64 overflow-auto rounded p-3 text-xs whitespace-pre-wrap"
          >{{ result.notes }}</pre>
        <p v-else class="text-muted-foreground text-sm">The release has no notes.</p>
        <p class="text-muted-foreground text-xs break-all">{{ result.url }}</p>
      </template>

      <DialogFooter class="gap-2 sm:flex-wrap">
        <Button v-if="error" variant="outline" :disabled="checking" @click="checkNow()">
          Try again
        </Button>
        <Button variant="outline" @click="close()">
          {{ result?.available ? "Later" : "Close" }}
        </Button>
        <Button v-if="result?.available" @click="download()">Open the download page</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
