<script setup lang="ts">
// ============================================================
// SurveyView - The current burst side by side (N), up to four photos
// zoomed to the same relative point; the focus is the current photo
// ============================================================

import { computed, reactive, ref, watch } from 'vue'
import { useElementSize } from '@vueuse/core'
import { convertFileSrc } from '@tauri-apps/api/core'
import { useSessionStore } from '@/stores/sessionStore'
import { useViewStore } from '@/stores/viewStore'
import { useContextMenu } from '@/composables/useContextMenu'
import { groupRange } from '@/core/grouping'
import { surveyGrid, surveyPaneTransform, surveyWindow } from '@/core/survey'
import { fitWidth, paneSize, type Size } from '@/core/zoom'
import { photoName } from '@/core/feedback'
import { PhotoStatus, type PhotoPair } from '@/types'
import StatusBadge from './StatusBadge.vue'

/** A press that moves further than this is a drag (panning), not a click */
const CLICK_SLOP_PX = 4

const session = useSessionStore()
const view = useViewStore()
const contextMenu = useContextMenu()

const rootRef = ref<HTMLElement | null>(null)
const { width: rootWidth, height: rootHeight } = useElementSize(rootRef)

const burst = computed(() => groupRange(session.groups, session.currentIndex))

const visible = computed(() => surveyWindow(view.surveyStart, burst.value, session.currentIndex))

// Remember the window so it only slides when the focus reaches its edge
watch(
  () => visible.value.start,
  (start) => (view.surveyStart = start),
  { immediate: true },
)

const panes = computed(() => {
  const { start, end } = visible.value
  return session.pairs.slice(start, end).map((pair, i) => ({ pair, index: start + i }))
})

const grid = computed(() => surveyGrid(panes.value.length))
const pane = computed(() =>
  paneSize({ width: rootWidth.value, height: rootHeight.value }, grid.value),
)

/** Natural size of each photo once loaded, by pair id */
const naturalSizes = reactive(new Map<string, Size>())

function handleLoad(pair: PhotoPair, e: Event) {
  const img = e.target as HTMLImageElement
  naturalSizes.set(pair.id, { width: img.naturalWidth, height: img.naturalHeight })
}

function fitOf(pair: PhotoPair | null | undefined): Size {
  const natural = pair ? naturalSizes.get(pair.id) : undefined
  if (!natural) return { width: 0, height: 0 }
  const width = fitWidth(natural, pane.value, view.rotation)
  return { width, height: (width * natural.height) / natural.width }
}

function imageStyle(pair: PhotoPair) {
  const fit = fitOf(pair)
  if (!fit.width) return { maxWidth: '100%', maxHeight: '100%' }
  const state = { zoom: view.zoomLevel, offsetX: view.zoomOffsetX, offsetY: view.zoomOffsetY }
  const t = surveyPaneTransform(fit, fitOf(session.currentPair), state)
  return {
    maxWidth: 'none',
    maxHeight: 'none',
    width: `${t.width}px`,
    transform: `translate(${t.x}px, ${t.y}px) rotate(${view.rotation}deg)`,
  }
}

const windowLabel = computed(() => {
  const { start, end } = burst.value
  if (end - start <= panes.value.length) return ''
  return `${visible.value.start - start + 1}–${visible.value.end - start} / ${end - start}`
})

let pressedAt: { x: number; y: number } | null = null

function handleMouseDown(e: MouseEvent) {
  pressedAt = { x: e.clientX, y: e.clientY }
}

/** Clicking a pane focuses it, unless the press was a drag to pan */
function handleClick(index: number, e: MouseEvent) {
  const moved = pressedAt
    ? Math.hypot(e.clientX - pressedAt.x, e.clientY - pressedAt.y) > CLICK_SLOP_PX
    : false
  if (!moved) session.goTo(index)
}
</script>

<template>
  <div
    ref="rootRef"
    class="absolute inset-0 grid"
    :style="{
      gridTemplateColumns: `repeat(${grid.cols}, minmax(0, 1fr))`,
      gridTemplateRows: `repeat(${grid.rows}, minmax(0, 1fr))`,
    }"
  >
    <div
      v-for="{ pair, index } in panes"
      :key="pair.id"
      class="relative flex items-center justify-center overflow-hidden border border-white/5"
      :class="index === session.currentIndex ? 'ring-2 ring-inset ring-sift-accent z-10' : ''"
      @mousedown="handleMouseDown"
      @click="handleClick(index, $event)"
      @contextmenu.prevent="contextMenu.show($event, pair)"
    >
      <!-- Thumbnail until the photo itself has loaded -->
      <img
        v-if="pair.thumbnailPath && !naturalSizes.has(pair.id)"
        :src="convertFileSrc(pair.thumbnailPath)"
        class="absolute inset-0 w-full h-full object-contain blur-sm opacity-60"
        draggable="false"
      />
      <img
        :src="convertFileSrc(pair.jpgPath)"
        class="shrink-0 transition-opacity duration-200"
        :class="pair.status === PhotoStatus.Deleted ? 'opacity-35' : 'opacity-100'"
        :style="imageStyle(pair)"
        draggable="false"
        decoding="async"
        @load="handleLoad(pair, $event)"
      />

      <div class="absolute top-2 left-2 right-2 z-20 flex items-center gap-2">
        <span
          class="px-2 py-0.5 rounded-md bg-black/50 backdrop-blur-sm text-[10px] font-mono truncate"
          :class="index === session.currentIndex ? 'text-white/90' : 'text-white/50'"
        >
          {{ photoName(pair) }}
        </span>
        <StatusBadge :status="pair.status" class="ml-auto shrink-0" />
      </div>
    </div>

    <!-- Which part of a long burst is in view -->
    <div
      v-if="windowLabel"
      class="absolute top-2 left-1/2 -translate-x-1/2 z-30 px-2.5 py-1 rounded-lg bg-black/60 backdrop-blur-sm text-[11px] font-mono text-sift-burst"
    >
      {{ windowLabel }}
    </div>
  </div>
</template>
