<script setup lang="ts">
// ============================================================
// AfOverlay - Where the camera focused, drawn over a photo (P)
// Laid exactly over the <img> by `frameStyle`, so it zooms, pans and
// rotates with it; marks are placed in shares of the photo and sized in
// screen px, so they stay crisp and visible at any zoom
// ============================================================

import { computed } from 'vue'
import { afMarks } from '@/core/afPoint'
import type { AfInfo } from '@/types'

/** A focus box never shrinks below this on screen, so a small one stays visible */
const MIN_AREA_PX = 14

const props = defineProps<{
  info?: AfInfo
  frameStyle: Record<string, string> | null
}>()

const marks = computed(() => afMarks(props.info))

function percent(share: number): string {
  return `${share * 100}%`
}
</script>

<template>
  <div
    v-if="marks && frameStyle"
    class="absolute left-1/2 top-1/2 z-[25] pointer-events-none"
    :style="frameStyle"
  >
    <span
      v-for="(point, i) in marks.points"
      :key="i"
      class="af-mark af-point"
      :style="{ left: percent(point.x), top: percent(point.y) }"
    />
    <span
      v-if="marks.area"
      class="af-mark af-area"
      :style="{
        left: percent(marks.area.x),
        top: percent(marks.area.y),
        width: `max(${MIN_AREA_PX}px, ${percent(marks.area.width)})`,
        height: `max(${MIN_AREA_PX}px, ${percent(marks.area.height)})`,
      }"
    />
  </div>
</template>

<style scoped>
.af-mark {
  position: absolute;
  transform: translate(-50%, -50%);
  /* A dark rim keeps the green readable on bright and green backgrounds */
  box-shadow:
    0 0 0 1px rgb(0 0 0 / 0.55),
    inset 0 0 0 1px rgb(0 0 0 / 0.55);
}

.af-area {
  border: 2px solid #4ade80;
}

.af-point {
  width: 10px;
  height: 10px;
  border: 1.5px solid rgb(74 222 128 / 0.8);
}
</style>
