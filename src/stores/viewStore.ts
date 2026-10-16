// ============================================================
// Sift - View Store (Pinia)
// UI state: current view, panels, zoom
// ============================================================

import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import type { AppView } from '@/types'
import { PhotoStatus } from '@/types'
import { clampZoom, isActualSize, zoomAt, type Point } from '@/core/zoom'
import { SortOrder, isSortOrder } from '@/core/sorting'
import { DEFAULT_BURST_GAP_SECONDS, isBurstGapSeconds } from '@/core/grouping'
import { setOrganizing as reportOrganizing } from '@/services/tauriCommands'

const PANE_CENTER: Point = { x: 0, y: 0 }
const TOAST_DURATION_MS = 3000
const STAY_AFTER_MARK_KEY = 'sift.stayAfterMark'
const SORT_ORDER_KEY = 'sift.sortOrder'
const BURST_GAP_KEY = 'sift.burstGapSeconds'
const SHOW_AF_POINT_KEY = 'sift.showAfPoint'

/** Viewer preferences, not session progress: unreadable storage means the default */
function loadPreference(key: string): string | null {
  try {
    return localStorage.getItem(key)
  } catch (e) {
    console.warn(`[Sift] Failed to read preference ${key}`, e)
    return null
  }
}

function savePreference(key: string, value: string): void {
  try {
    localStorage.setItem(key, value)
  } catch (e) {
    console.warn(`[Sift] Failed to save preference ${key}`, e)
  }
}

function loadSortOrder(): SortOrder {
  const saved = loadPreference(SORT_ORDER_KEY)
  return isSortOrder(saved) ? saved : SortOrder.CaptureTime
}

function loadBurstGapSeconds(): number {
  const saved = Number(loadPreference(BURST_GAP_KEY) ?? NaN)
  return isBurstGapSeconds(saved) ? saved : DEFAULT_BURST_GAP_SECONDS
}

export type ToastType = 'star' | 'delete' | 'undo' | 'info'

export const useViewStore = defineStore('view', () => {
  const currentView = ref<AppView>('welcome')
  const showExifPanel = ref(false)
  const showArchiveDialog = ref(false)
  const showSummaryCard = ref(false)
  const showShortcutHelp = ref(false)
  /** Marking stays on the photo instead of moving on, for comparing a burst in place */
  const stayAfterMark = ref(loadPreference(STAY_AFTER_MARK_KEY) === 'true')
  /** Order photos are shown in; the session re-sorts when it changes */
  const sortOrder = ref<SortOrder>(loadSortOrder())
  /** Neighbours taken at most this far apart form a burst; 0 turns grouping off */
  const burstGapSeconds = ref(loadBurstGapSeconds())
  /** Where the camera focused is drawn over the photo */
  const showAfPoint = ref(loadPreference(SHOW_AF_POINT_KEY) === 'true')
  /** Offset that centres the current photo's focus; null without one or with the overlay off */
  const afFocusOffset = ref<Point | null>(null)
  /** Archiving, exporting or deleting is running: nothing may close or leave it */
  const isOrganizing = ref(false)

  // Zoom state
  const zoomLevel = ref(1)
  const zoomOffsetX = ref(0)
  const zoomOffsetY = ref(0)
  const rotation = ref(0)
  /** Zoom level for 1 image px = 1 device px; null until the image is measured */
  const actualZoom = ref<number | null>(null)
  /** Last cursor position relative to the hovered pane's center */
  const zoomAnchor = ref<Point>({ ...PANE_CENTER })

  // Compare mode state
  const compareMode = ref(false)
  const compareIndex = ref<number | null>(null)

  // Survey (N): the current burst side by side
  const surveyMode = ref(false)
  /** First photo in view when the burst is longer than the survey shows */
  const surveyStart = ref<number | null>(null)

  /** I in compare or survey lays each photo's settings over its pane instead of the drawer */
  const showsShotStrips = computed(
    () => showExifPanel.value && (compareMode.value || surveyMode.value),
  )

  // Toast
  const toastMessage = ref('')
  const toastType = ref<ToastType>('info')
  const toastVisible = ref(false)
  let toastTimer: ReturnType<typeof setTimeout> | undefined

  // Filter gallery
  const filterCategory = ref<PhotoStatus | null>(null)

  /** Switching views leaves the old session behind, including its summary */
  function setView(view: AppView) {
    currentView.value = view
    showSummaryCard.value = false
  }

  function toggleExifPanel() {
    showExifPanel.value = !showExifPanel.value
  }

  /** Also tells Rust, which then refuses to close the window or quit */
  function setOrganizing(active: boolean) {
    isOrganizing.value = active
    reportOrganizing(active).catch((e) =>
      console.warn('[Sift] Failed to report organizing state:', e),
    )
  }

  function toggleArchiveDialog() {
    showArchiveDialog.value = !showArchiveDialog.value
  }

  function openShortcutHelp() {
    showShortcutHelp.value = true
  }

  function closeShortcutHelp() {
    showShortcutHelp.value = false
  }

  function resetZoom() {
    zoomLevel.value = 1
    zoomOffsetX.value = 0
    zoomOffsetY.value = 0
    rotation.value = 0
  }

  function setZoom(next: number, anchor: Point = PANE_CENTER) {
    const state = zoomAt(
      { zoom: zoomLevel.value, offsetX: zoomOffsetX.value, offsetY: zoomOffsetY.value },
      clampZoom(next, actualZoom.value),
      anchor,
    )
    zoomLevel.value = state.zoom
    zoomOffsetX.value = state.offsetX
    zoomOffsetY.value = state.offsetY
  }

  function zoomToActualSize(anchor: Point = zoomAnchor.value) {
    if (actualZoom.value === null) return
    setZoom(actualZoom.value, anchor)
  }

  /** 100% centred on where the camera focused; false (nothing done) without a focus to go to */
  function zoomToFocus(): boolean {
    const offset = afFocusOffset.value
    if (!showAfPoint.value || !offset || actualZoom.value === null) return false
    zoomLevel.value = clampZoom(actualZoom.value, actualZoom.value)
    zoomOffsetX.value = offset.x
    zoomOffsetY.value = offset.y
    return true
  }

  /** Lightroom-style Z: toggle between fit and 1:1 at the cursor */
  function toggleActualSize(anchor: Point = zoomAnchor.value) {
    if (isActualSize(zoomLevel.value, actualZoom.value)) {
      setZoom(1)
    } else {
      zoomToActualSize(anchor)
    }
  }

  function rotateBy(delta: number) {
    rotation.value += delta
  }

  /** Show a toast, replacing any current one (its timer must not hide this one early) */
  function showToast(message: string, type: ToastType = 'info', durationMs = TOAST_DURATION_MS) {
    toastMessage.value = message
    toastType.value = type
    toastVisible.value = true
    clearTimeout(toastTimer)
    toastTimer = setTimeout(() => {
      toastVisible.value = false
    }, durationMs)
  }

  function toggleStayAfterMark() {
    stayAfterMark.value = !stayAfterMark.value
    savePreference(STAY_AFTER_MARK_KEY, String(stayAfterMark.value))
    showToast(stayAfterMark.value ? '标记后停在当前照片' : '标记后自动前进')
  }

  function toggleAfPoint() {
    showAfPoint.value = !showAfPoint.value
    savePreference(SHOW_AF_POINT_KEY, String(showAfPoint.value))
    showToast(showAfPoint.value ? '显示对焦点（1 放大到对焦点）' : '隐藏对焦点')
  }

  function setSortOrder(order: SortOrder) {
    sortOrder.value = order
    savePreference(SORT_ORDER_KEY, order)
  }

  function setBurstGapSeconds(seconds: number) {
    burstGapSeconds.value = seconds
    savePreference(BURST_GAP_KEY, String(seconds))
  }

  function toggleCompare(currentIdx: number) {
    if (compareMode.value) {
      exitCompare()
    } else {
      exitSurvey()
      compareMode.value = true
      compareIndex.value = currentIdx
      resetZoom()
    }
  }

  function exitCompare() {
    compareMode.value = false
    compareIndex.value = null
  }

  /** Survey and compare are exclusive; panes change size, so the zoom starts at fit */
  function toggleSurvey() {
    if (surveyMode.value) {
      exitSurvey()
    } else {
      exitCompare()
      surveyMode.value = true
      surveyStart.value = null
      resetZoom()
    }
  }

  function exitSurvey() {
    if (!surveyMode.value) return
    surveyMode.value = false
    surveyStart.value = null
    resetZoom()
  }

  function swapCompare(currentIdx: number) {
    if (compareMode.value) {
      compareIndex.value = currentIdx
    }
  }

  function openFilterGallery(category: PhotoStatus) {
    filterCategory.value = category
  }

  function closeFilterGallery() {
    filterCategory.value = null
  }

  return {
    currentView,
    showExifPanel,
    showArchiveDialog,
    showSummaryCard,
    showShortcutHelp,
    stayAfterMark,
    showAfPoint,
    afFocusOffset,
    sortOrder,
    burstGapSeconds,
    isOrganizing,
    setOrganizing,
    zoomLevel,
    zoomOffsetX,
    zoomOffsetY,
    rotation,
    actualZoom,
    zoomAnchor,
    compareMode,
    compareIndex,
    surveyMode,
    showsShotStrips,
    surveyStart,
    toastMessage,
    toastType,
    toastVisible,
    filterCategory,
    setView,
    toggleExifPanel,
    toggleArchiveDialog,
    openShortcutHelp,
    closeShortcutHelp,
    resetZoom,
    setZoom,
    zoomToActualSize,
    zoomToFocus,
    toggleActualSize,
    rotateBy,
    showToast,
    toggleStayAfterMark,
    toggleAfPoint,
    setSortOrder,
    setBurstGapSeconds,
    toggleCompare,
    exitCompare,
    toggleSurvey,
    exitSurvey,
    swapCompare,
    openFilterGallery,
    closeFilterGallery,
  }
})
