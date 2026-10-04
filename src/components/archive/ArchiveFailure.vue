<script setup lang="ts">
// ============================================================
// ArchiveFailure - Shown when archiving or exporting stops partway:
// what went wrong, what was already done, where it got stuck, what to do
// ============================================================

import { TriangleAlert } from 'lucide-vue-next'
import type { FailureReport } from '@/core/organizeError'

defineProps<{ report: FailureReport }>()

const emit = defineEmits<{
  /** Back to the options, to try again */
  retry: []
  /** Leave the folder; reopening it picks up the files as they are now */
  home: []
}>()
</script>

<template>
  <div class="py-2" role="alert">
    <div
      class="mx-auto w-14 h-14 rounded-full bg-sift-delete/15 flex items-center justify-center mb-3"
    >
      <TriangleAlert :size="28" class="text-sift-delete" />
    </div>
    <h3 class="text-lg font-bold text-white text-center">{{ report.title }}</h3>
    <p class="text-sm text-sift-delete text-center mt-1 mb-4">{{ report.reason }}</p>

    <section class="bg-sift-card/60 rounded-xl p-3 mb-3 text-xs">
      <p class="text-[11px] text-sift-muted mb-1">已完成</p>
      <ul class="space-y-0.5 text-white/90">
        <li v-for="line in report.done" :key="line">{{ line }}</li>
      </ul>
      <template v-if="report.stuckFile">
        <p class="text-[11px] text-sift-muted mt-2 mb-1">卡在</p>
        <p class="font-mono text-white truncate" :title="report.stuckPath ?? undefined">
          {{ report.stuckFile }}
        </p>
      </template>
    </section>

    <section class="bg-sift-card/40 rounded-xl p-3 mb-3">
      <p class="text-[11px] text-sift-muted mb-1">下一步</p>
      <p class="text-xs text-white/90 leading-relaxed">{{ report.advice }}</p>
    </section>

    <details class="mb-5 text-xs">
      <summary class="cursor-pointer text-sift-muted hover:text-white">详情</summary>
      <pre
        class="mt-2 max-h-32 overflow-auto whitespace-pre-wrap break-all rounded-lg bg-black/30 p-2 font-mono text-[11px] text-sift-muted select-text"
        >{{ report.detail }}</pre>
    </details>

    <button
      v-if="report.canRetry"
      class="w-full h-11 rounded-xl bg-sift-card text-white text-sm hover:bg-sift-border transition-colors btn-spring"
      @click="emit('retry')"
    >
      返回
    </button>
    <button
      v-else
      class="w-full h-11 rounded-xl bg-sift-card text-white text-sm hover:bg-sift-border transition-colors btn-spring"
      @click="emit('home')"
    >
      返回首页
    </button>
  </div>
</template>
