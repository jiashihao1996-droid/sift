<script setup lang="ts">
// ============================================================
// HistogramView - RGB + luma histogram with clipping indicators
// ============================================================

import { computed } from 'vue'
import { formatClipRatio, isClipWarning, type Histogram } from '@/core/histogram'
import HistogramPlot from './HistogramPlot.vue'

const props = defineProps<{
  histogram: Histogram | null
  isLoading: boolean
  hasError: boolean
  /** Shown under the plot: what the histogram was computed from */
  sourceLabel: string
}>()

const highlightWarning = computed(
  () => !!props.histogram && isClipWarning(props.histogram.highlightClipRatio),
)
const shadowWarning = computed(
  () => !!props.histogram && isClipWarning(props.histogram.shadowClipRatio),
)

const summary = computed(() => {
  const h = props.histogram
  if (!h) return '直方图'
  return `直方图：高光溢出 ${formatClipRatio(h.highlightClipRatio)}，暗部溢出 ${formatClipRatio(h.shadowClipRatio)}`
})
</script>

<template>
  <div class="space-y-2">
    <HistogramPlot
      class="h-[100px]"
      role="img"
      :aria-label="summary"
      :histogram="histogram"
      :is-loading="isLoading"
      :has-error="hasError"
    />

    <div v-if="histogram" class="flex items-center justify-between text-[11px] tabular-nums">
      <span :class="shadowWarning ? 'text-sift-accent' : 'text-sift-muted/70'">
        暗部溢出 {{ formatClipRatio(histogram.shadowClipRatio) }}
      </span>
      <span :class="highlightWarning ? 'text-sift-delete' : 'text-sift-muted/70'">
        高光溢出 {{ formatClipRatio(histogram.highlightClipRatio) }}
      </span>
    </div>
    <p class="text-[10px] text-sift-muted/50">{{ sourceLabel }}</p>
  </div>
</template>
