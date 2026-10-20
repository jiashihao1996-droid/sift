<script setup lang="ts">
// ============================================================
// ThumbnailStrip - Bottom thumbnail filmstrip with status bars
// Navigation via gradient edge hotzone buttons
// Only the thumbnails in view (plus a margin) are rendered, each placed at
// its position from core/stripLayout.ts: a 20,000-photo folder would
// otherwise re-render 20,000 tiles on every key press.
// ============================================================

import { computed, ref, shallowRef, watch, nextTick, onBeforeUnmount } from 'vue'
import { ChevronLeft, ChevronRight } from '@lucide/vue'
import { useSessionStore } from '@/stores/sessionStore'
import { useViewStore } from '@/stores/viewStore'
import { PhotoStatus, type PhotoPair } from '@/types'
import { convertFileSrc } from '@tauri-apps/api/core'
import ContextMenu from '@/components/common/ContextMenu.vue'
import { useContextMenu } from '@/composables/useContextMenu'
import { burstEdges } from '@/core/grouping'
import { layoutStrip, scrollToCenter, visibleRange } from '@/core/stripLayout'

const session = useSessionStore()
const view = useViewStore()
const contextMenu = useContextMenu()

/** Tile width (w-16), gap between tiles (gap-1.5), extra gap around a burst (ml-2.5), padding (px-16) */
const METRICS = { itemWidth: 64, gap: 6, burstGap: 10, padding: 64 }
/** Tiles rendered beyond each edge of the view, so scrolling never shows a gap */
const OVERSCAN = 10
/** Farther than this many view widths, jump instead of animating the scroll */
const SMOOTH_SCROLL_VIEWS = 2

/** Group bounds change only when the order or gap does, not on each mark */
const edges = computed(() => burstEdges(session.groups))

const layout = computed(() =>
  layoutStrip(session.pairs.length, (i) => edges.value[i]?.isSpacedBefore ?? false, METRICS),
)

const scrollContainer = ref<HTMLElement | null>(null)
const scrollLeft = ref(0)
const viewportWidth = ref(0)
const isHovering = ref(false)

const range = computed(() =>
  visibleRange(layout.value, scrollLeft.value, viewportWidth.value, METRICS.itemWidth, OVERSCAN),
)

/** The tiles to render; reads only their pairs, so marks elsewhere cost nothing here */
const visible = computed(() => {
  const { start, end } = range.value
  const tiles: { pair: PhotoPair; index: number }[] = []
  for (let index = start; index < end; index++) tiles.push({ pair: session.pairs[index], index })
  return tiles
})

function scrollToActive() {
  const container = scrollContainer.value
  if (!container) return
  const target = scrollToCenter(
    layout.value,
    session.currentIndex,
    container.clientWidth,
    METRICS.itemWidth,
  )
  const isFar =
    Math.abs(target - container.scrollLeft) > container.clientWidth * SMOOTH_SCROLL_VIEWS
  container.scrollTo({ left: target, behavior: isFar ? 'instant' : 'smooth' })
}

/** Page left: scroll by 60% of the container width */
function pageLeft() {
  const container = scrollContainer.value
  if (!container) return
  const scrollAmount = container.clientWidth * 0.6
  container.scrollBy({ left: -scrollAmount, behavior: 'smooth' })
}

/** Page right: scroll by 60% of the container width */
function pageRight() {
  const container = scrollContainer.value
  if (!container) return
  const scrollAmount = container.clientWidth * 0.6
  container.scrollBy({ left: scrollAmount, behavior: 'smooth' })
}

function handleClick(index: number) {
  session.goTo(index)
}

/** Whether we can scroll left */
const canScrollLeft = ref(false)
/** Whether we can scroll right */
const canScrollRight = ref(false)

function updateScrollState() {
  const container = scrollContainer.value
  if (!container) return
  scrollLeft.value = container.scrollLeft
  viewportWidth.value = container.clientWidth
  canScrollLeft.value = container.scrollLeft > 0
  canScrollRight.value = container.scrollLeft + container.clientWidth < container.scrollWidth - 1
}

function getStatusColor(status: PhotoStatus): string {
  switch (status) {
    case PhotoStatus.Starred:
      return 'bg-sift-star'
    case PhotoStatus.Deleted:
      return 'bg-sift-delete'
    default:
      return ''
  }
}

function getDominantColor(pair: Pick<PhotoPair, 'dominantColor'>): string {
  return pair.dominantColor || '#2a2a2a'
}

watch(
  () => session.currentIndex,
  async () => {
    await nextTick()
    scrollToActive()
  },
)

/** The container comes and goes with the photos; follow it */
const resizeObserver = shallowRef<ResizeObserver | null>(null)
watch(scrollContainer, async (container, previous) => {
  previous?.removeEventListener('scroll', updateScrollState)
  resizeObserver.value?.disconnect()
  if (!container) return
  container.addEventListener('scroll', updateScrollState, { passive: true })
  resizeObserver.value = new ResizeObserver(updateScrollState)
  resizeObserver.value.observe(container)
  updateScrollState()
  await nextTick()
  scrollToActive()
})

onBeforeUnmount(() => {
  scrollContainer.value?.removeEventListener('scroll', updateScrollState)
  resizeObserver.value?.disconnect()
})
</script>

<template>
  <div
    v-if="session.pairs.length > 0"
    class="fixed bottom-status left-0 right-0 h-strip z-40 bg-[#121212] border-t border-white/5"
    @mouseenter="isHovering = true"
    @mouseleave="isHovering = false"
  >
    <!-- Fade mask container -->
    <div class="relative w-full h-full thumbnail-strip-mask">
      <!-- Left page hotzone -->
      <div
        v-show="canScrollLeft"
        class="absolute left-0 top-0 bottom-0 w-14 z-10 flex items-center justify-center cursor-pointer bg-linear-to-r/srgb from-[#121212] via-[#121212]/60 to-transparent transition-opacity duration-200"
        :class="isHovering ? 'opacity-100' : 'opacity-0'"
        @click="pageLeft"
      >
        <ChevronLeft
          :size="16"
          :stroke-width="1.5"
          class="text-white/50 hover:text-white/90 transition-colors"
        />
      </div>

      <!-- Right page hotzone -->
      <div
        v-show="canScrollRight"
        class="absolute right-0 top-0 bottom-0 w-14 z-10 flex items-center justify-center cursor-pointer bg-linear-to-l/srgb from-[#121212] via-[#121212]/60 to-transparent transition-opacity duration-200"
        :class="isHovering ? 'opacity-100' : 'opacity-0'"
        @click="pageRight"
      >
        <ChevronRight
          :size="16"
          :stroke-width="1.5"
          class="text-white/50 hover:text-white/90 transition-colors"
        />
      </div>

      <!-- Scrollable thumbnail list: a strip as wide as all tiles, holding only those in view -->
      <div ref="scrollContainer" class="h-full overflow-x-auto thumbnail-scroll">
        <div class="relative h-full" :style="{ width: `${layout.width}px` }">
          <div
            v-for="{ pair, index } in visible"
            :key="pair.id"
            class="absolute top-1/2 -translate-y-1/2"
            :style="{ left: `${layout.lefts[index]}px` }"
          >
            <!-- Burst bar: joins the members of a burst under their thumbnails -->
            <div
              v-if="edges[index]?.isInBurst"
              class="absolute -bottom-1.5 left-0 h-0.5 bg-sift-burst/70"
              :class="[
                edges[index].isFirst ? 'rounded-l-full' : '',
                edges[index].isLast ? 'right-0 rounded-r-full' : '-right-1.5',
              ]"
            />
            <div
              class="relative h-11 w-16 rounded-md overflow-hidden cursor-pointer transition-[opacity,scale,box-shadow] duration-200 ease-out group"
              :class="[
                index === session.currentIndex
                  ? 'ring-2 ring-sift-accent ring-offset-1 ring-offset-[#121212] opacity-100 scale-105'
                  : view.compareMode && index === view.compareIndex
                    ? 'ring-2 ring-blue-400 ring-offset-1 ring-offset-[#121212] opacity-90 scale-[1.03]'
                    : 'opacity-50 hover:opacity-80',
              ]"
              @click="handleClick(index)"
              @contextmenu.prevent="contextMenu.show($event, pair)"
            >
              <!-- Dominant color placeholder -->
              <div class="absolute inset-0" :style="{ backgroundColor: getDominantColor(pair) }" />

              <!-- Thumbnail once generated; until then the colour above stands in, since
                 loading the full-size photo for a 64px tile costs a 24MP decode -->
              <img
                v-if="pair.thumbnailPath"
                :src="convertFileSrc(pair.thumbnailPath)"
                :alt="`Photo ${index + 1}`"
                loading="lazy"
                decoding="async"
                class="absolute inset-0 w-full h-full object-cover"
                @error="($event.target as HTMLImageElement).style.display = 'none'"
              />

              <!-- Status color bar -->
              <div
                v-if="pair.status !== PhotoStatus.Unprocessed"
                class="absolute bottom-0 left-0 right-0 h-1 rounded-b-md"
                :class="getStatusColor(pair.status)"
              />

              <!-- Compare base badge -->
              <div
                v-if="view.compareMode && index === view.compareIndex"
                class="absolute top-0 left-0 right-0 flex justify-center z-10"
              >
                <span class="text-[8px] bg-blue-500/80 text-white px-1 rounded-b leading-tight"
                  >基准</span
                >
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Context Menu -->
    <ContextMenu />
  </div>
</template>

<style scoped>
.thumbnail-strip-mask {
  mask-image: linear-gradient(
    to right,
    transparent 0px,
    black 60px,
    black calc(100% - 60px),
    transparent 100%
  );
  -webkit-mask-image: linear-gradient(
    to right,
    transparent 0px,
    black 60px,
    black calc(100% - 60px),
    transparent 100%
  );
}

.thumbnail-scroll {
  scrollbar-width: none;
  -ms-overflow-style: none;
}

.thumbnail-scroll::-webkit-scrollbar {
  display: none;
}
</style>
