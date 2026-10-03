// ============================================================
// Sift - Session Store (Pinia)
// Core state: photo pairs, current index, operations, undo
// ============================================================

import { defineStore } from 'pinia'
import { ref, computed, watch } from 'vue'
import {
  PhotoStatus,
  type PhotoPair,
  type ScanResult,
  type ThumbnailResult,
  type FolderProgress,
  type UndoAction,
  type SlideDirection,
} from '@/types'
import {
  scanFolder,
  generateThumbnails,
  cleanupCache,
  loadProgress,
  saveProgress,
  clearProgress,
} from '@/services/tauriCommands'
import {
  countByStatus,
  indexAfterMark,
  planBatches,
  progressPercent,
  slideDirectionTo,
  toggledStatus,
  type MarkType,
} from '@/core/review'
import { hasMarks, restoreProgress, snapshotProgress } from '@/core/progress'
import { createLatestWriter } from '@/utils/latestWriter'

/** First thumbnail batch covers the visible viewport; the rest stream in chunks */
const THUMBNAIL_FIRST_BATCH = 10
const THUMBNAIL_CHUNK_SIZE = 50

/** A folder with no marks left has nothing worth resuming */
function writeProgress(progress: FolderProgress): Promise<void> {
  return hasMarks(progress) ? saveProgress(progress) : clearProgress(progress.folder)
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
  const slideDirection = ref<SlideDirection>('left')
  const isGeneratingThumbnails = ref(false)
  const markActionCount = ref(0)
  /** Marks brought back from the last visit to this folder */
  const restoredCount = ref(0)

  const progressWriter = createLatestWriter(writeProgress, (e) =>
    console.warn('[Sift] Failed to save progress:', e),
  )

  // ---- Getters ----
  const currentPair = computed(() => pairs.value[currentIndex.value] || null)

  const totalCount = computed(() => pairs.value.length)

  const counts = computed(() => countByStatus(pairs.value))
  const starredCount = computed(() => counts.value[PhotoStatus.Starred])
  const deletedCount = computed(() => counts.value[PhotoStatus.Deleted])
  const skippedCount = computed(() => counts.value[PhotoStatus.Skipped])
  const unprocessedCount = computed(() => counts.value[PhotoStatus.Unprocessed])

  /** Active (non-deleted) pairs for browsing */
  const activePairs = computed(() => pairs.value.filter((p) => p.status !== PhotoStatus.Deleted))

  /** Progress percentage */
  const progress = computed(() => progressPercent(counts.value))

  const isLastPhoto = computed(() => currentIndex.value >= pairs.value.length - 1)

  /** All photos have been processed */
  const hasReachedEnd = computed(() => unprocessedCount.value === 0)

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
    isScanning.value = true
    folderPath.value = path
    try {
      const result = await scanFolder(path)
      const restored = restoreProgress(result.pairs, await loadProgressOrNone(path))
      scanResult.value = result
      pairs.value = restored.pairs
      currentIndex.value = restored.currentIndex
      restoredCount.value = restored.restoredCount
      undoStack.value = []
    } finally {
      isScanning.value = false // 扫描完立即停止转圈，不等缩略图
    }

    // 分批生成缩略图（后台异步，不阻塞 startScan 返回）
    if (pairs.value.length > 0) {
      void generateAllThumbnails(pairs.value.map((p) => ({ id: p.id, jpgPath: p.jpgPath })))
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

  /** Navigate to next photo */
  function goNext() {
    if (currentIndex.value < pairs.value.length - 1) {
      slideDirection.value = 'left'
      currentIndex.value++
    }
  }

  /** Move to `index`, sliding in `direction`; no-op if already there */
  function moveTo(index: number, direction: SlideDirection) {
    if (index === currentIndex.value) return
    slideDirection.value = direction
    currentIndex.value = index
  }

  /** Navigate to previous photo */
  function goPrev() {
    if (currentIndex.value > 0) {
      slideDirection.value = 'right'
      currentIndex.value--
    }
  }

  /** Navigate to specific index */
  function goTo(index: number) {
    if (index >= 0 && index < pairs.value.length) {
      slideDirection.value = slideDirectionTo(currentIndex.value, index)
      currentIndex.value = index
    }
  }

  /**
   * Toggle a mark on the current photo, then advance.
   * Returns whether the photo ended up marked, or null if there is no photo.
   * Marking records an undo entry; toggling a mark off does not.
   */
  function toggleMark(type: MarkType): boolean | null {
    const idx = currentIndex.value
    const pair = pairs.value[idx]
    if (!pair) return null

    const status = toggledStatus(pair.status, type)
    const isMarked = status !== PhotoStatus.Unprocessed
    if (isMarked) {
      undoStack.value.push({ type, index: idx, previousStatus: pair.status })
      markActionCount.value++
    }
    pairs.value[idx] = { ...pair, status }
    const next = indexAfterMark(pairs.value, idx)
    moveTo(next, slideDirectionTo(idx, next))
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

  /** Toggle skip on current photo */
  function markSkip(): 'skipped' | 'unskipped' {
    return toggleMark('skip') === false ? 'unskipped' : 'skipped'
  }

  /** Undo last action */
  function undo() {
    const action = undoStack.value.pop()
    if (!action) return

    const pair = pairs.value[action.index]
    if (pair) {
      pairs.value[action.index] = { ...pair, status: action.previousStatus }
      goTo(action.index)
    }
  }

  /** Save marks and position whenever either changes, so quitting midway loses nothing */
  watch(
    () => [currentIndex.value, pairs.value.map((p) => p.status)],
    () => {
      if (!folderPath.value || pairs.value.length === 0) return
      progressWriter.schedule(snapshotProgress(folderPath.value, pairs.value, currentIndex.value))
    },
  )

  /** Drop restored marks and start the folder over (also forgets the saved progress) */
  function restartSession() {
    pairs.value = pairs.value.map((p) =>
      p.status === PhotoStatus.Unprocessed ? p : { ...p, status: PhotoStatus.Unprocessed },
    )
    currentIndex.value = 0
    undoStack.value = []
    restoredCount.value = 0
  }

  /** Forget the saved progress of the current folder, e.g. once it is archived */
  function forgetProgress() {
    if (!folderPath.value) return
    progressWriter.schedule({ folder: folderPath.value, currentKey: null, marks: {} })
  }

  /** Reset session */
  function resetSession() {
    // Clean up cache directories
    cleanupCache().catch((e) => console.warn('Cache cleanup failed:', e))

    folderPath.value = ''
    pairs.value = []
    currentIndex.value = 0
    isScanning.value = false
    scanResult.value = null
    undoStack.value = []
    restoredCount.value = 0
  }

  return {
    // State
    folderPath,
    pairs,
    currentIndex,
    isScanning,
    scanResult,
    undoStack,
    slideDirection,
    isGeneratingThumbnails,
    markActionCount,
    restoredCount,
    // Getters
    currentPair,
    totalCount,
    starredCount,
    deletedCount,
    skippedCount,
    unprocessedCount,
    activePairs,
    progress,
    isLastPhoto,
    hasReachedEnd,
    // Actions
    startScan,
    goNext,
    goPrev,
    goTo,
    markStar,
    markDelete,
    markSkip,
    undo,
    restartSession,
    forgetProgress,
    resetSession,
    /** Resolves once pending progress writes are done (tests) */
    progressSaved: progressWriter.idle,
  }
})
