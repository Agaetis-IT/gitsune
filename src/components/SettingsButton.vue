<script setup lang="ts">
import { Settings } from "@lucide/vue";
import { useSettings } from "@/composables/useSettings";

const { openSettings } = useSettings();
</script>

<template>
  <button
    class="text-muted-foreground hover:bg-accent hover:text-foreground flex items-center gap-2 rounded px-2 py-1.5 text-sm"
    title="Settings (Cmd+,)"
    @click="openSettings"
  >
    <Settings class="size-4" />
    Settings
  </button>
</template>
