<script setup lang="ts">
import { Settings } from "@lucide/vue";
import { useSettings } from "@/composables/useSettings";
import { APP_VERSION_LABEL } from "@/lib/appVersion";

const { openSettings } = useSettings();
</script>

<template>
  <div>
    <button
      class="text-muted-foreground hover:bg-accent hover:text-foreground flex items-center gap-2 rounded px-2 py-1.5 text-sm"
      title="Settings (Cmd+,)"
      @click="openSettings"
    >
      <Settings class="size-4" />
      Settings
    </button>
    <p class="text-muted-foreground px-2 text-xs">Gitsune {{ APP_VERSION_LABEL }}</p>
  </div>
</template>
