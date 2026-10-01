<script setup lang="ts">
// ============================================================
// RecentFolders - Quick reopen list on the welcome page
// ============================================================

import { Folder, X } from 'lucide-vue-next'
import { extractFileName } from '@/utils/path'

defineProps<{ folders: string[] }>()

const emit = defineEmits<{
  open: [path: string]
  remove: [path: string]
}>()

function folderName(path: string): string {
  return extractFileName(path.replace(/[\\/]+$/, '')) || path
}
</script>

<template>
  <section v-if="folders.length > 0" class="w-full mt-6" aria-labelledby="recent-folders-title">
    <h2
      id="recent-folders-title"
      class="text-[11px] font-medium text-sift-muted uppercase tracking-wider mb-2 px-1"
    >
      最近打开
    </h2>
    <ul class="space-y-1">
      <li v-for="path in folders" :key="path" class="group relative">
        <button
          type="button"
          class="w-full flex items-center gap-3 pl-3 pr-10 py-2 rounded-lg text-left hover:bg-white/[0.04] focus-visible:bg-white/[0.06] focus-visible:outline-none transition-colors"
          :title="path"
          @click="emit('open', path)"
        >
          <Folder
            :size="14"
            class="shrink-0 text-sift-muted group-hover:text-sift-accent transition-colors"
          />
          <span class="text-sm text-sift-text shrink-0 max-w-[45%] truncate">
            {{ folderName(path) }}
          </span>
          <span class="text-[11px] text-sift-muted/60 truncate">{{ path }}</span>
        </button>
        <button
          type="button"
          class="absolute right-2 top-1/2 -translate-y-1/2 p-1 rounded-md text-sift-muted/60 opacity-0 group-hover:opacity-100 focus-visible:opacity-100 hover:text-white hover:bg-white/10 transition-all"
          :aria-label="`从最近列表移除 ${folderName(path)}`"
          title="从列表移除"
          @click="emit('remove', path)"
        >
          <X :size="12" />
        </button>
      </li>
    </ul>
  </section>
</template>
