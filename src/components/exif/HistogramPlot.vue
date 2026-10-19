<script setup lang="ts">
// ============================================================
// HistogramPlot - RGB + luma histogram with clipping corners; fills its box
// ============================================================

import { computed } from 'vue'
import { histogramPath, isClipWarning, plotPeak, type Histogram } from '@/core/histogram'

const props = defineProps<{
  histogram: Histogram | null
  isLoading: boolean
  hasError: boolean
}>()

const PLOT_HEIGHT = 100

const paths = computed(() => {
  const h = props.histogram
  if (!h) return null
  const peak = plotPeak([h.red, h.green, h.blue, h.luma])
  return {
    luma: histogramPath(h.luma, peak, PLOT_HEIGHT),
    red: histogramPath(h.red, peak, PLOT_HEIGHT),
    green: histogramPath(h.green, peak, PLOT_HEIGHT),
    blue: histogramPath(h.blue, peak, PLOT_HEIGHT),
  }
})

const highlightWarning = computed(
  () => !!props.histogram && isClipWarning(props.histogram.highlightClipRatio),
)
const shadowWarning = computed(
  () => !!props.histogram && isClipWarning(props.histogram.shadowClipRatio),
)
</script>

<template>
  <div class="relative rounded-md bg-black/40 border border-white/5 overflow-hidden">
    <svg
      v-if="paths"
      class="absolute inset-0 w-full h-full"
      viewBox="0 0 256 100"
      preserveAspectRatio="none"
      aria-hidden="true"
    >
      <path :d="paths.luma" fill="rgba(255,255,255,0.14)" />
      <g style="mix-blend-mode: screen">
        <path :d="paths.red" fill="rgba(239,68,68,0.55)" />
        <path :d="paths.green" fill="rgba(34,197,94,0.55)" />
        <path :d="paths.blue" fill="rgba(59,130,246,0.6)" />
      </g>
    </svg>

    <div v-else-if="isLoading" class="absolute inset-0 skeleton-pulse" />

    <p
      v-else-if="hasError"
      class="absolute inset-0 flex items-center justify-center text-[11px] text-sift-muted"
    >
      无法读取图像
    </p>

    <!-- Clipping indicators (Lightroom convention: blue = shadows, red = highlights) -->
    <span
      class="clip-corner left-0 border-l-10 border-r-0"
      :class="shadowWarning ? 'border-l-sift-accent' : 'border-l-white/15'"
      aria-hidden="true"
    />
    <span
      class="clip-corner right-0 border-r-10 border-l-0"
      :class="highlightWarning ? 'border-r-sift-delete' : 'border-r-white/15'"
      aria-hidden="true"
    />
  </div>
</template>

<style scoped>
.clip-corner {
  position: absolute;
  top: 0;
  width: 0;
  height: 0;
  border-bottom: 10px solid transparent;
  transition: border-color 200ms ease;
}
</style>
