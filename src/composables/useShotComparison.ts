// ============================================================
// Sift - Shooting settings of the photos on screen, lined up
// Compare (C): base image and current photo, measured from the base.
// Survey (N): the burst in view, measured from the focus.
// Otherwise: the current photo alone.
// ============================================================

import { computed, shallowRef, watch, type Ref } from 'vue'
import { useSessionStore } from '@/stores/sessionStore'
import { useViewStore } from '@/stores/viewStore'
import { groupRange } from '@/core/grouping'
import { surveyWindow } from '@/core/survey'
import {
  captureOffsets,
  compareShots,
  formatCaptureOffset,
  formatClockTime,
} from '@/core/shotCompare'
import type { ExifData, PhotoPair } from '@/types'
import { cachedExif, loadExif } from './usePhotoExif'

export interface ShotsOnScreen {
  pairs: PhotoPair[]
  /** Index in `pairs` that the others are compared with */
  reference: number
}

export function useShotComparison(isEnabled: Ref<boolean>) {
  const session = useSessionStore()
  const view = useViewStore()

  const shots = computed<ShotsOnScreen>(() => {
    const current = session.currentPair
    if (!current) return { pairs: [], reference: 0 }
    if (view.surveyMode) {
      const burst = groupRange(session.groups, session.currentIndex)
      const { start, end } = surveyWindow(view.surveyStart, burst, session.currentIndex)
      return {
        pairs: session.pairs.slice(start, end),
        reference: session.currentIndex - start,
      }
    }
    const base = view.compareMode && view.compareIndex !== null && session.pairs[view.compareIndex]
    if (base) return { pairs: [base, current], reference: 0 }
    return { pairs: [current], reference: 0 }
  })

  /** EXIF by pair id; filled as reads finish */
  const exifById = shallowRef(new Map<string, ExifData | null>())
  let generation = 0

  watch(
    [() => shots.value.pairs, isEnabled],
    async ([pairs, enabled]) => {
      const thisGeneration = ++generation
      if (!enabled) return
      const known = new Map<string, ExifData | null>()
      for (const pair of pairs) {
        const cached = cachedExif(pair)
        if (cached !== undefined) known.set(pair.id, cached)
      }
      exifById.value = known
      if (known.size === pairs.length) return
      const results = await Promise.all(pairs.map((pair) => loadExif(pair)))
      if (thisGeneration !== generation) return
      exifById.value = new Map(pairs.map((pair, i) => [pair.id, results[i]]))
    },
    { immediate: true },
  )

  const exifs = computed(() => shots.value.pairs.map((p) => exifById.value.get(p.id) ?? null))
  const rows = computed(() => compareShots(exifs.value, shots.value.reference))

  /** Reference photo: its clock time; the others: how long after it */
  const timeLabels = computed(() => {
    const { pairs, reference } = shots.value
    const offsets = captureOffsets(pairs, reference)
    return pairs.map((pair, i) => {
      if (i === reference) return pair.captureTime ? formatClockTime(pair.captureTime.localMs) : ''
      const offset = offsets[i]
      return offset === null ? '' : formatCaptureOffset(offset)
    })
  })

  /** What the settings strip over `pair`'s pane shows; null when it is not on screen */
  function stripOf(pair: PhotoPair | null | undefined) {
    if (!pair) return null
    const column = shots.value.pairs.findIndex((p) => p.id === pair.id)
    if (column < 0) return null
    return {
      jpgPath: pair.jpgPath,
      rows: rows.value,
      column,
      timeLabel: timeLabels.value[column] ?? '',
      isReference: column === shots.value.reference,
    }
  }

  return { shots, rows, timeLabels, stripOf }
}

export type ShotComparison = ReturnType<typeof useShotComparison>
