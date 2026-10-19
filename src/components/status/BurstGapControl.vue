<script setup lang="ts">
// ============================================================
// BurstGapControl - Status bar button choosing how far apart
// neighbouring photos may be to count as one burst
// ============================================================

import { ref } from 'vue'
import { Layers } from '@lucide/vue'
import { useViewStore } from '@/stores/viewStore'
import { BURST_GAP_SECONDS } from '@/core/grouping'

const view = useViewStore()
const isOpen = ref(false)

function gapLabel(seconds: number): string {
  return seconds === 0 ? '关' : `${seconds} 秒`
}

function choose(seconds: number) {
  view.setBurstGapSeconds(seconds)
  isOpen.value = false
}
</script>

<template>
  <div class="relative">
    <button
      class="h-6 flex items-center gap-1 rounded-sm px-2 text-xs text-sift-muted hover:text-white hover:bg-white/5 transition-colors cursor-pointer"
      :class="{ 'text-white bg-white/5': isOpen }"
      title="连拍分组：拍摄时间相隔不超过这个间隔的相邻照片算一组"
      @click="isOpen = !isOpen"
    >
      <Layers :size="12" />
      连拍 {{ gapLabel(view.burstGapSeconds) }}
    </button>

    <template v-if="isOpen">
      <!-- Click anywhere else to close -->
      <div class="fixed inset-0 z-40" @click="isOpen = false" />
      <div
        class="absolute bottom-full left-0 mb-2 z-50 w-44 p-1.5 rounded-lg bg-sift-surface border border-white/10 shadow-xl"
        role="menu"
      >
        <p class="px-2 pt-1 pb-1.5 text-[11px] leading-snug text-sift-muted">
          相邻照片拍摄时间相隔不超过
        </p>
        <button
          v-for="seconds in BURST_GAP_SECONDS"
          :key="seconds"
          role="menuitemradio"
          :aria-checked="seconds === view.burstGapSeconds"
          class="w-full flex items-center justify-between rounded-sm px-2 py-1 text-xs transition-colors cursor-pointer"
          :class="
            seconds === view.burstGapSeconds
              ? 'bg-sift-accent/15 text-sift-accent'
              : 'text-white/70 hover:bg-white/5 hover:text-white'
          "
          @click="choose(seconds)"
        >
          {{ seconds === 0 ? '关（不分组）' : gapLabel(seconds) }}
        </button>
      </div>
    </template>
  </div>
</template>
