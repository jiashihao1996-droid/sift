// ============================================================
// Sift - Focus Point Composable
// While the overlay is on: reads where the camera focused for the photos
// on screen and the next few, publishes the offset that centres the
// current photo's focus (for `1`), and when photos are switched while
// zoomed in, moves the view to the new photo's focus.
// ============================================================

import { computed, watch, watchEffect, type Ref } from 'vue'
import { useSessionStore } from '@/stores/sessionStore'
import { useViewStore } from '@/stores/viewStore'
import { useAfPointStore } from '@/stores/afPointStore'
import { afMarks, centerOffset, focusCenter } from '@/core/afPoint'
import { preloadOrder } from '@/core/preload'
import type { Size } from '@/core/zoom'

export function useAfFocus(fitSize: Ref<Size>, isLoading: Ref<boolean>) {
  const session = useSessionStore()
  const view = useViewStore()
  const af = useAfPointStore()

  // The photos in view first, then the ones browsing is likely to reach
  watchEffect(() => {
    if (!view.showAfPoint) return
    const index = session.currentIndex
    const ahead = preloadOrder(index, session.pairs.length, 1).map((i) => session.pairs[i])
    const base = view.compareIndex === null ? null : session.pairs[view.compareIndex]
    af.load([session.currentPair, base, ...ahead])
  })

  const currentInfo = computed(() => af.infoOf(session.currentPair))

  /** Offset centring the current photo's focus; null until it and the photo are known */
  const focusOffset = computed(() => {
    const center = focusCenter(afMarks(currentInfo.value))
    if (!view.showAfPoint || !center || isLoading.value || !fitSize.value.width) return null
    return centerOffset(center, fitSize.value, view.rotation)
  })

  watchEffect(() => {
    view.afFocusOffset = focusOffset.value
  })

  // Registered after useImageZoom's own switch watcher, so a view it reset
  // (rotated photos start at fit) is not followed
  let shouldFollow = false
  watch(
    () => session.currentIndex,
    () => {
      shouldFollow = view.showAfPoint && view.zoomLevel > 1
    },
  )

  // Post-flush: after useImageZoom has carried the zoom over to the new photo
  watch(
    focusOffset,
    (offset) => {
      if (!shouldFollow || !offset) return
      shouldFollow = false
      view.zoomOffsetX = offset.x
      view.zoomOffsetY = offset.y
    },
    { flush: 'post' },
  )

  return { currentInfo }
}
