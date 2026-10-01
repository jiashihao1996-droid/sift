// ============================================================
// Sift - Session Store (Pinia)
// Core state: photo pairs, current index, operations, undo
// ============================================================

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import {
  PhotoStatus,
  type PhotoPair,
  type ScanResult,
  type ThumbnailResult,
  type UndoAction,
  type SlideDirection,
} from '@/types'
import { scanFolder, generateThumbnails, cleanupCache } from '@/services/tauriCommands'
import {
  countByStatus,
  indexAfterMark,
  planBatches,
  progressPercent,
  toggledStatus,
  type MarkType,
} from '@/core/review'

/** First thumbnail batch covers the visible viewport; the rest stream in chunks */
const THUMBNAIL_FIRST_BATCH = 10
const THUMBNAIL_CHUNK_SIZE = 50

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
      scanResult.value = result
      pairs.value = result.pairs
      currentIndex.value = 0
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
      slideDirection.value = index > currentIndex.value ? 'left' : 'right'
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
    moveTo(indexAfterMark(pairs.value, idx), 'left')
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
    resetSession,
  }
})
