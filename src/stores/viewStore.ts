// ============================================================
// Sift - View Store (Pinia)
// UI state: current view, panels, zoom
// ============================================================

import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { AppView } from '@/types'
import { PhotoStatus } from '@/types'
import { clampZoom, isActualSize, zoomAt, type Point } from '@/core/zoom'

const PANE_CENTER: Point = { x: 0, y: 0 }

export const useViewStore = defineStore('view', () => {
  const currentView = ref<AppView>('welcome')
  const showExifPanel = ref(false)
  const showArchiveDialog = ref(false)
  const showSummaryCard = ref(false)
  const showShortcutHelp = ref(false)

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

  // Toast
  const toastMessage = ref('')
  const toastType = ref<'star' | 'delete' | 'undo' | 'info'>('info')
  const toastVisible = ref(false)

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

  function showToast(message: string, type: 'star' | 'delete' | 'undo' | 'info' = 'info') {
    toastMessage.value = message
    toastType.value = type
    toastVisible.value = true
    setTimeout(() => {
      toastVisible.value = false
    }, 3000)
  }

  function toggleCompare(currentIdx: number) {
    if (compareMode.value) {
      exitCompare()
    } else {
      compareMode.value = true
      compareIndex.value = currentIdx
      resetZoom()
    }
  }

  function exitCompare() {
    compareMode.value = false
    compareIndex.value = null
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
    zoomLevel,
    zoomOffsetX,
    zoomOffsetY,
    rotation,
    actualZoom,
    zoomAnchor,
    compareMode,
    compareIndex,
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
    toggleActualSize,
    rotateBy,
    showToast,
    toggleCompare,
    exitCompare,
    swapCompare,
    openFilterGallery,
    closeFilterGallery,
  }
})
