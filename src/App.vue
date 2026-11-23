<script setup lang="ts">
import { Button } from "@/components/ui/button";
import { useTheme, type ThemeMode } from "@/composables/useTheme";

const { mode, setMode } = useTheme();
const modes: ThemeMode[] = ["system", "light", "dark"];
</script>

<template>
  <main
    class="bg-background text-foreground flex h-screen flex-col items-center justify-center gap-6"
  >
    <h1 class="text-3xl font-semibold tracking-tight">git-hydra</h1>
    <div class="flex gap-2">
      <Button
        v-for="m in modes"
        :key="m"
        :variant="mode === m ? 'default' : 'outline'"
        @click="setMode(m)"
      >
        {{ m }}
      </Button>
    </div>
  </main>
</template>
