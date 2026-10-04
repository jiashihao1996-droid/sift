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
  movesOnAfterMark,
  planBatches,
  positionPercent,
  toggledStatus,
  type MarkType,
} from '@/core/review'
import { emptyProgress, hasProgress, restoreProgress, snapshotProgress } from '@/core/progress'
import type { MarkChange } from '@/core/feedback'
import { createLatestWriter } from '@/utils/latestWriter'
import { useViewStore } from './viewStore'

/** First thumbnail batch covers the visible viewport; the rest stream in chunks */
const THUMBNAIL_FIRST_BATCH = 10
const THUMBNAIL_CHUNK_SIZE = 50

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

  const progressWriter = createLatestWriter(writeProgress, (e) =>
    console.warn('[Sift] Failed to save progress:', e),
  )

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
      isResumed.value = restored.isResumed
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

  /** Navigate to next photo; on the last one, report reaching the end instead */
  function goNext() {
    if (currentIndex.value < pairs.value.length - 1) {
      currentIndex.value++
    } else if (pairs.value.length > 0) {
      endReachedCount.value++
    }
  }

  /** Navigate to previous photo */
  function goPrev() {
    if (currentIndex.value > 0) {
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
    if (!pair) return null

    const status = toggledStatus(pair.status, type)
    const isMarked = status !== PhotoStatus.Unprocessed
    const { stayAfterMark } = useViewStore()
    undoStack.value.push({ type, index: idx, previousStatus: pair.status })
    pairs.value[idx] = { ...pair, status }
    lastMark.value = { type, isMarked, index: idx, seq: (lastMark.value?.seq ?? 0) + 1 }
    if (movesOnAfterMark(status, stayAfterMark) && isLastPhoto.value) endReachedCount.value++
    currentIndex.value = indexAfterMark(status, idx, pairs.value.length, stayAfterMark)
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
    const action = undoStack.value.pop()
    if (!action) return false

    const pair = pairs.value[action.index]
    if (pair) {
      pairs.value[action.index] = { ...pair, status: action.previousStatus }
      currentIndex.value = action.index
    }
    return true
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
    isResumed.value = false
  }

  /** Forget the saved progress of the current folder, e.g. once it is archived */
  function forgetProgress() {
    if (!folderPath.value) return
    progressWriter.schedule(emptyProgress(folderPath.value))
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
    isResumed.value = false
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
    // Getters
    currentPair,
    totalCount,
    starredCount,
    deletedCount,
    unprocessedCount,
    progress,
    isLastPhoto,
    // Actions
    startScan,
    goNext,
    goPrev,
    goTo,
    markStar,
    markDelete,
    undo,
    restartSession,
    forgetProgress,
    resetSession,
    /** Resolves once pending progress writes are done (tests) */
    progressSaved: progressWriter.idle,
  }
})
