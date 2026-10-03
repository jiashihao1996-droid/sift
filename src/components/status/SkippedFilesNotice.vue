<script setup lang="ts">
// ============================================================
// SkippedFilesNotice - Lists photo files the scan found but could not
// load, so none of them disappear without the user knowing
// ============================================================

import { computed, ref, watch } from 'vue'
import { TriangleAlert, X } from 'lucide-vue-next'
import { useSessionStore } from '@/stores/sessionStore'
import { describeSkipped } from '@/core/scanReport'

const session = useSessionStore()

const lines = computed(() => describeSkipped(session.scanResult?.skipped ?? []))
const isDismissed = ref(false)
const isExpanded = ref(false)

// A new scan brings the notice back
watch(
  () => session.scanResult,
  () => {
    isDismissed.value = false
    isExpanded.value = false
  },
)
</script>

<template>
  <div
    v-if="lines.length > 0 && !isDismissed"
    class="fixed top-14 right-4 z-40 w-80 max-w-[calc(100vw-2rem)] rounded-xl border border-sift-star/30 bg-sift-surface/95 backdrop-blur-lg shadow-lg text-sm"
    role="status"
  >
    <div class="flex items-center gap-2 px-3 py-2">
      <TriangleAlert :size="14" class="shrink-0 text-sift-star" />
      <button
        class="flex-1 text-left text-white/90 hover:text-white"
        :aria-expanded="isExpanded"
        @click="isExpanded = !isExpanded"
      >
        {{ lines.length }} 个文件未载入
        <span class="text-xs text-sift-muted">{{ isExpanded ? '收起' : '查看' }}</span>
      </button>
      <button
        class="p-1 rounded hover:bg-white/10 text-sift-muted"
        aria-label="关闭"
        @click="isDismissed = true"
      >
        <X :size="12" />
      </button>
    </div>
    <ul v-if="isExpanded" class="max-h-60 overflow-y-auto border-t border-white/5 px-3 py-2">
      <li v-for="line in lines" :key="line.path" class="py-1" :title="line.path">
        <div class="truncate font-mono text-xs text-white/90">{{ line.name }}</div>
        <div class="text-xs text-sift-muted">{{ line.reason }}</div>
      </li>
    </ul>
  </div>
</template>
