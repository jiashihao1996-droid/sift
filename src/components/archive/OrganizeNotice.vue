<script setup lang="ts">
// ============================================================
// OrganizeNotice - What a delete-only run or an export just did, shown on
// the options page so archiving can follow
// ============================================================

import { CheckCircle } from 'lucide-vue-next'
import { showInFolder } from '@/services/tauriCommands'

defineProps<{ message: string; folder: string | null }>()

function reveal(folder: string) {
  showInFolder(folder).catch((e) => console.warn('[Sift] Failed to reveal folder:', e))
}
</script>

<template>
  <div
    class="bg-sift-success/10 border border-sift-success/20 rounded-lg px-3 py-2 mb-5 flex items-center gap-2"
    role="status"
  >
    <CheckCircle :size="14" class="text-sift-success shrink-0" />
    <div class="min-w-0 flex-1">
      <p class="text-xs text-white/90">{{ message }}</p>
      <p v-if="folder" class="text-[11px] text-sift-muted truncate" :title="folder">
        {{ folder }}
      </p>
    </div>
    <button
      v-if="folder"
      class="text-[11px] text-sift-accent hover:text-white shrink-0 transition-colors"
      @click="reveal(folder)"
    >
      在文件夹中显示
    </button>
  </div>
</template>
