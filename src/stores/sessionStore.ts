// ============================================================
// Sift - Session Store (Pinia)
// Core state: photo pairs, current index, operations, undo
// ============================================================

import { defineStore } from 'pinia'
import { ref, computed, shallowRef, toRaw, watch } from 'vue'
import {
  PhotoStatus,
  type PhotoPair,
  type ScanResult,
  type ThumbnailResult,
  type FolderProgress,
  type UndoAction,
} from '@/types'
import {
  scanFolder,
  generateThumbnails,
  loadProgress,
  saveProgress,
  clearProgress,
} from '@/services/tauriCommands'
import {
  countByStatus,
  indexAfterMark,
  movesOnAfterMark,
  planBatches,
  positionPercent,
  toggledStatus,
  type MarkType,
} from '@/core/review'
import { nearestFirst } from '@/core/preload'
import { fileNameRanks, sortPairs } from '@/core/sorting'
import { stepWithinGroup } from '@/core/survey'
import {
  groupPairs,
  groupPosition,
  groupRange,
  nextGroupStart,
  planGroupMark,
  prevGroupTarget,
  type BurstGroups,
  type GroupMarkMode,
  type GroupMarkPlan,
} from '@/core/grouping'
import { withoutPairs } from '@/core/archiveFlow'
import {
  defaultStatus,
  emptyProgress,
  hasProgress,
  restoreProgress,
  snapshotProgress,
} from '@/core/progress'
import type { MarkChange } from '@/core/feedback'
import { createLatestWriter } from '@/utils/latestWriter'
import { createDebouncedWriter } from '@/utils/debouncedWriter'
import { useViewStore } from './viewStore'

/** First thumbnail batch covers the visible viewport; the rest stream in chunks */
const THUMBNAIL_FIRST_BATCH = 10
const THUMBNAIL_CHUNK_SIZE = 50

/**
 * Progress is saved once marking and moving pause for half a second, and at
 * least every 2 seconds while they go on: a fully marked 20,000-photo folder
 * is a 1.5MB write. Leaving the folder, organizing photos, hiding or closing
 * the window write it at once (`flushProgress`).
 */
const PROGRESS_SAVE_TIMING = { delayMs: 500, maxWaitMs: 2000 }

/** A folder with nothing to restore is forgotten rather than saved */
function writeProgress(progress: FolderProgress): Promise<void> {
  return hasProgress(progress) ? saveProgress(progress) : clearProgress(progress.folder)
}

/** Saved progress is a convenience: failing to read it must not block the scan */
async function loadProgressOrNone(folder: string): Promise<FolderProgress | null> {
  try {
    return await loadProgress(folder)
  } catch (e) {
    console.warn('[Sift] Failed to load saved progress:', e)
    return null
  }
}

export const useSessionStore = defineStore('session', () => {
  // ---- State ----
  const folderPath = ref<string>('')
  const pairs = ref<PhotoPair[]>([])
  const currentIndex = ref(0)
  const isScanning = ref(false)
  const scanResult = ref<ScanResult | null>(null)
  const undoStack = ref<UndoAction[]>([])
  const isGeneratingThumbnails = ref(false)
  /** The latest mark key press, for feedback; `seq` tells repeats apart */
  const lastMark = ref<(MarkChange & { index: number; seq: number }) | null>(null)
  /** Bumped each time the user tries to move on from the last photo */
  const endReachedCount = ref(0)
  /** Marks brought back from the last visit to this folder */
  const restoredCount = ref(0)
  /** Whether the last visit's marks or position were brought back */
  const isResumed = ref(false)
  /**
   * The folder was archived: its photos moved, so the session no longer
   * matches the disk. Marks, undo and saved progress are frozen.
   */
  const isArchived = ref(false)

  /** Each pair's place in file-name order, from the scan; not reactive */
  let fileRanks = new Map<string, number>()
  const view = useViewStore()

  const progressWriter = createDebouncedWriter(
    createLatestWriter(writeProgress, (e) => console.warn('[Sift] Failed to save progress:', e)),
    PROGRESS_SAVE_TIMING,
  )
  /**
   * Bumped whenever a mark changes in place; together with a new pairs array
   * and the current index it tells the progress saver that something changed,
   * without reading every pair's status on each key press
   */
  const marksVersion = ref(0)

  // ---- Getters ----
  const currentPair = computed(() => pairs.value[currentIndex.value] || null)

  const totalCount = computed(() => pairs.value.length)

  const counts = computed(() => countByStatus(pairs.value))
  const starredCount = computed(() => counts.value[PhotoStatus.Starred])
  const deletedCount = computed(() => counts.value[PhotoStatus.Deleted])
  const unprocessedCount = computed(() => counts.value[PhotoStatus.Unprocessed])

  /** How far through the folder the current photo is */
  const progress = computed(() => positionPercent(currentIndex.value, pairs.value.length))

  const isLastPhoto = computed(() => currentIndex.value >= pairs.value.length - 1)

  /**
   * Burst groups of the current order. Worked out again only when the order
   * or the gap changes (a new pairs array, not a mark), so marking never pays
   * for regrouping or for redrawing the strip's group bounds.
   */
  const groups = shallowRef<BurstGroups>({ starts: [], groupOf: [] })
  watch(
    [() => pairs.value, () => view.burstGapSeconds],
    ([list, gapSeconds]) => {
      groups.value = groupPairs(toRaw(list), gapSeconds * 1000)
    },
    { immediate: true, flush: 'sync' },
  )

  /** Where the current photo sits in its burst; null when it stands alone */
  const currentGroupPosition = computed(() => groupPosition(groups.value, currentIndex.value))

  // ---- Actions ----

  /** Apply thumbnail results to pairs in place (Map indexed, O(n)) */
  function applyThumbnails(thumbs: ThumbnailResult[]) {
    const idToIndex = new Map<string, number>()
    pairs.value.forEach((p, i) => idToIndex.set(p.id, i))
    for (const thumb of thumbs) {
      const idx = idToIndex.get(thumb.id)
      if (idx !== undefined) {
        pairs.value[idx].thumbnailPath = thumb.path
        pairs.value[idx].dominantColor = thumb.dominantColor
      }
    }
  }

  /** Scan a folder and populate pairs */
  async function startScan(path: string) {
    // The previous folder's last changes are saved before the session changes
    void progressWriter.flush()
    isScanning.value = true
    folderPath.value = path
    isArchived.value = false
    try {
      const result = await scanFolder(path)
      fileRanks = fileNameRanks(result.pairs)
      const sorted = sortPairs(result.pairs, view.sortOrder, fileRanks)
      const restored = restoreProgress(sorted, await loadProgressOrNone(path))
      scanResult.value = result
      pairs.value = restored.pairs
      currentIndex.value = restored.currentIndex
      restoredCount.value = restored.restoredCount
      isResumed.value = restored.isResumed
      undoStack.value = []
    } finally {
      isScanning.value = false // 扫描完立即停止转圈，不等缩略图
    }

    // 分批生成缩略图（后台异步，不阻塞 startScan 返回）
    if (pairs.value.length > 0) {
      const inputs = pairs.value.map((p) => ({ id: p.id, jpgPath: p.jpgPath }))
      void generateAllThumbnails(nearestFirst(inputs, currentIndex.value))
    }
  }

  async function generateAllThumbnails(inputs: { id: string; jpgPath: string }[]) {
    isGeneratingThumbnails.value = true
    try {
      // 每批完成立即更新 UI
      for (const batch of planBatches(inputs, THUMBNAIL_FIRST_BATCH, THUMBNAIL_CHUNK_SIZE)) {
        applyThumbnails(await generateThumbnails(batch))
      }
    } catch (e) {
      console.warn('Thumbnail generation failed:', e)
    } finally {
      isGeneratingThumbnails.value = false
    }
  }

  /**
   * Navigate to next photo; on the last one, report reaching the end instead.
   * A survey keeps the focus within its burst.
   */
  function goNext() {
    if (view.surveyMode) {
      currentIndex.value = stepWithinGroup(groups.value, currentIndex.value, 1)
    } else if (currentIndex.value < pairs.value.length - 1) {
      currentIndex.value++
    } else if (pairs.value.length > 0) {
      endReachedCount.value++
    }
  }

  /** Navigate to previous photo; a survey keeps the focus within its burst */
  function goPrev() {
    if (view.surveyMode) {
      currentIndex.value = stepWithinGroup(groups.value, currentIndex.value, -1)
    } else if (currentIndex.value > 0) {
      currentIndex.value--
    }
  }

  /** Navigate to specific index */
  function goTo(index: number) {
    if (index >= 0 && index < pairs.value.length) {
      currentIndex.value = index
    }
  }

  /**
   * Toggle a mark on the current photo: marking advances (unless the viewer is
   * set to stay after marking), unmarking stays. Every change is undoable.
   * Returns whether the photo ended up marked, or null if there is no photo.
   */
  function toggleMark(type: MarkType): boolean | null {
    const idx = currentIndex.value
    const pair = pairs.value[idx]
    if (!pair || isArchived.value) return null

    const status = toggledStatus(pair.status, type)
    const isMarked = status !== PhotoStatus.Unprocessed
    const { stayAfterMark, surveyMode } = view
    // A survey moves the focus within its burst and stops at the burst's end
    const end = surveyMode ? groupRange(groups.value, idx).end : pairs.value.length
    undoStack.value.push({
      pairId: pair.id,
      changes: [{ pairId: pair.id, previousStatus: pair.status }],
    })
    setStatus(idx, status)
    lastMark.value = { type, isMarked, index: idx, seq: (lastMark.value?.seq ?? 0) + 1 }
    if (movesOnAfterMark(status, stayAfterMark) && isLastPhoto.value && !surveyMode) {
      endReachedCount.value++
    }
    currentIndex.value = indexAfterMark(status, idx, end, stayAfterMark)
    return isMarked
  }

  /** Toggle star on current photo */
  function markStar(): 'starred' | 'unstarred' {
    return toggleMark('star') === false ? 'unstarred' : 'starred'
  }

  /** Toggle delete on current photo (only marks status, actual deletion happens during archive) */
  function markDelete(): 'deleted' | 'undeleted' {
    return toggleMark('delete') === false ? 'undeleted' : 'deleted'
  }

  /** Undo the last mark change and return to that photo; false if there was nothing to undo */
  function undo(): boolean {
    if (isArchived.value) return false
    const action = undoStack.value.pop()
    if (!action) return false

    for (const change of action.changes) {
      const index = indexOfPair(change.pairId)
      if (index !== null) setStatus(index, change.previousStatus)
    }
    const returnTo = indexOfPair(action.pairId)
    if (returnTo !== null) currentIndex.value = returnTo
    return true
  }

  /** `]`: first photo of the next burst; in the last one, report reaching the end */
  function goToNextGroup() {
    const target = nextGroupStart(groups.value, currentIndex.value)
    if (target !== null) {
      currentIndex.value = target
    } else if (pairs.value.length > 0) {
      endReachedCount.value++
    }
  }

  /** `[`: back to the start of this burst, or from there to the previous one */
  function goToGroupStart() {
    const target = prevGroupTarget(groups.value, currentIndex.value)
    if (target !== null) currentIndex.value = target
  }

  /**
   * Decide the current burst in one key (Shift+F / Shift+X), as one undo step.
   * Moves on to the next burst unless the viewer stays after marking.
   * Returns what changed, or null if there is no photo.
   */
  function markGroup(mode: GroupMarkMode): GroupMarkPlan | null {
    const idx = currentIndex.value
    const pair = pairs.value[idx]
    if (!pair || isArchived.value) return null

    const plan = planGroupMark(pairs.value, groups.value, idx, mode)
    if (plan.changes.length === 0) return plan
    undoStack.value.push({
      pairId: pair.id,
      changes: plan.changes.map(({ index }) => ({
        pairId: pairs.value[index].id,
        previousStatus: pairs.value[index].status,
      })),
    })
    for (const { index, status } of plan.changes) setStatus(index, status)
    if (!view.stayAfterMark) goToNextGroup()
    return plan
  }

  /** Change one photo's mark in place; every mark change goes through here */
  function setStatus(index: number, status: PhotoStatus) {
    pairs.value[index] = { ...pairs.value[index], status }
    marksVersion.value++
  }

  function indexOfPair(id: string): number | null {
    const index = pairs.value.findIndex((p) => p.id === id)
    return index === -1 ? null : index
  }

  /** Re-sort when the order changes, staying on the same photo and compare base */
  watch(
    () => view.sortOrder,
    (order) => {
      const currentId = currentPair.value?.id
      const compareId = view.compareIndex !== null ? pairs.value[view.compareIndex]?.id : undefined
      pairs.value = sortPairs(pairs.value, order, fileRanks)
      currentIndex.value = currentId === undefined ? 0 : (indexOfPair(currentId) ?? 0)
      if (compareId) view.compareIndex = indexOfPair(compareId)
    },
  )

  /**
   * Save marks and position after they change, so quitting midway loses
   * nothing. Marks change in place (marksVersion) or with a new pairs array
   * (scan, restart, sort); the array is watched shallowly, so thumbnails
   * arriving do not count.
   */
  watch([currentIndex, () => pairs.value, marksVersion], () => {
    if (!folderPath.value || pairs.value.length === 0 || isArchived.value) return
    const folder = folderPath.value
    progressWriter.schedule(() => snapshotProgress(folder, pairs.value, currentIndex.value))
  })

  /** Archiving, exporting or deleting starts from saved progress */
  watch(
    () => view.isOrganizing,
    (isOrganizing) => {
      if (isOrganizing) void progressWriter.flush()
    },
  )

  /** Write pending progress now, e.g. before the window closes or is hidden */
  function flushProgress(): Promise<void> {
    return progressWriter.flush()
  }

  /**
   * Put every mark back to its default (archived picks stay starred) and
   * start the folder over (also forgets the saved progress)
   */
  function restartSession() {
    if (isArchived.value) return
    pairs.value = pairs.value.map((p) => {
      const status = defaultStatus(p)
      return p.status === status ? p : { ...p, status }
    })
    currentIndex.value = 0
    undoStack.value = []
    restoredCount.value = 0
    isResumed.value = false
  }

  /**
   * Remove pairs whose files are gone, e.g. trashed by an archive that then
   * stopped. Undo is dropped too: the photos it would restore may be gone.
   */
  function dropPairs(ids: readonly string[]) {
    if (ids.length === 0) return
    const next = withoutPairs(pairs.value, currentIndex.value, ids)
    pairs.value = next.pairs
    currentIndex.value = next.currentIndex
    undoStack.value = []
  }

  /**
   * Freeze the session once `folder` is archived and forget its saved
   * progress. The folder is the one captured when archiving started, so the
   * right progress is cleared even if the session changed since.
   */
  function markArchived(folder: string) {
    isArchived.value = true
    progressWriter.writeNow(emptyProgress(folder))
  }

  /** Reset session */
  function resetSession() {
    // The folder's last changes are saved before the session is cleared
    void progressWriter.flush()
    // The thumbnail and preview caches are kept: reopening a folder reuses
    // them, and Rust trims them to their size limit

    folderPath.value = ''
    pairs.value = []
    currentIndex.value = 0
    isScanning.value = false
    scanResult.value = null
    undoStack.value = []
    restoredCount.value = 0
    isResumed.value = false
    isArchived.value = false
  }

  return {
    // State
    folderPath,
    pairs,
    currentIndex,
    isScanning,
    scanResult,
    undoStack,
    isGeneratingThumbnails,
    lastMark,
    endReachedCount,
    restoredCount,
    isResumed,
    isArchived,
    // Getters
    currentPair,
    totalCount,
    starredCount,
    deletedCount,
    unprocessedCount,
    progress,
    isLastPhoto,
    groups,
    currentGroupPosition,
    // Actions
    startScan,
    goNext,
    goPrev,
    goTo,
    markStar,
    markDelete,
    markGroup,
    goToNextGroup,
    goToGroupStart,
    undo,
    restartSession,
    markArchived,
    dropPairs,
    resetSession,
    flushProgress,
  }
})
