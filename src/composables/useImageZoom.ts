// ============================================================
// Sift - Image Zoom Composable
// width controls render size (native res), translate for pan
// Container flex centers the image; no manual left/top calc
// ============================================================

import { ref, computed, watch, watchEffect, onMounted, onUnmounted, type Ref } from 'vue'
import { useDevicePixelRatio } from '@vueuse/core'
import { useViewStore } from '@/stores/viewStore'
import { useSessionStore } from '@/stores/sessionStore'
import {
  actualSizeZoom,
  applyZoomCarry,
  carryZoom,
  fitWidth as computeFitWidth,
  panByScroll,
  paneAnchor,
  paneSize,
  SINGLE_PANE,
  settleOnDetents,
  zoomPercent as computeZoomPercent,
  type DetentHold,
  type Point,
  type ZoomCarry,
} from '@/core/zoom'
import { surveyCount, surveyGrid } from '@/core/survey'
import { wheelIntent, wheelZoomFactor, type WheelInput } from '@/core/wheel'
import { IS_MAC } from '@/utils/platform'

const ZOOM_TRANSITION = '260ms cubic-bezier(0.22, 1, 0.36, 1)'
/** Wheel events closer than this belong to one gesture (they arrive every ~8–33 ms) */
const GESTURE_GAP_MS = 200

export function useImageZoom(
  containerRef: Ref<HTMLElement | null>,
  naturalWidth: Ref<number>,
  naturalHeight: Ref<number>,
) {
  const view = useViewStore()
  const session = useSessionStore()

  const isDragging = ref(false)
  /** A pinch, wheel or scroll is under way: it follows the fingers, so no transition */
  const isGesturing = ref(false)
  const dragStart = ref({ x: 0, y: 0 })
  const offsetStart = ref({ x: 0, y: 0 })

  // Reactive container size via ResizeObserver
  const containerWidth = ref(0)
  const containerHeight = ref(0)
  let resizeObserver: ResizeObserver | null = null

  function updateContainerSize() {
    const el = containerRef.value
    if (el) {
      containerWidth.value = el.clientWidth
      containerHeight.value = el.clientHeight
    }
  }

  onMounted(() => {
    updateContainerSize()
    resizeObserver = new ResizeObserver(() => updateContainerSize())
    if (containerRef.value) {
      resizeObserver.observe(containerRef.value)
    }
    document.addEventListener('mousemove', handleMouseMove)
    document.addEventListener('mouseup', handleMouseUp)
  })

  onUnmounted(() => {
    clearTimeout(gestureTimer)
    resizeObserver?.disconnect()
    document.removeEventListener('mousemove', handleMouseMove)
    document.removeEventListener('mouseup', handleMouseUp)
  })

  watch(containerRef, (el) => {
    resizeObserver?.disconnect()
    if (el) {
      updateContainerSize()
      resizeObserver?.observe(el)
    }
  })

  const dpr = useDevicePixelRatio().pixelRatio

  /** Panes the viewer is split into: compare is 2×1, a survey depends on the burst */
  const grid = computed(() => {
    if (view.surveyMode) return surveyGrid(surveyCount(session.groups, session.currentIndex))
    return view.compareMode ? { cols: 2, rows: 1 } : SINGLE_PANE
  })

  /** Fit width of the <img> at zoom=1 in its pane (rotation-aware) */
  const fitWidth = computed(() =>
    computeFitWidth(
      { width: naturalWidth.value, height: naturalHeight.value },
      paneSize({ width: containerWidth.value, height: containerHeight.value }, grid.value),
      view.rotation,
    ),
  )

  /** Fitted image size at zoom = 1, unrotated (zoom is only carried unrotated) */
  const fitSize = computed(() => ({
    width: fitWidth.value,
    height: naturalWidth.value ? (fitWidth.value * naturalHeight.value) / naturalWidth.value : 0,
  }))

  watchEffect(() => {
    view.actualZoom = actualSizeZoom(naturalWidth.value, fitWidth.value, dpr.value)
  })

  /** Explicit width so zoom renders at native resolution; CSS constraints until measured */
  const imageStyle = computed(() => {
    const fw = fitWidth.value
    const rot = view.rotation
    const tr =
      isDragging.value || isGesturing.value
        ? 'none'
        : `transform ${ZOOM_TRANSITION}, width ${ZOOM_TRANSITION}`

    if (!fw) {
      return {
        maxWidth: '100%',
        maxHeight: '100%',
        width: 'auto',
        transform: `rotate(${rot}deg)`,
        transition: tr,
        willChange: 'auto' as const,
      }
    }

    return {
      maxWidth: 'none',
      maxHeight: 'none',
      width: `${fw * view.zoomLevel}px`,
      transform: `translate(${view.zoomOffsetX * view.zoomLevel}px, ${view.zoomOffsetY * view.zoomLevel}px) rotate(${rot}deg)`,
      transition: tr,
      willChange: view.zoomLevel === 1 ? ('auto' as const) : ('transform' as const),
    }
  })

  /**
   * A box laid exactly over the <img>, for marks drawn on the photo: same
   * size, centred in the pane, moved and rotated with it. Null until measured.
   */
  const overlayStyle = computed(() => {
    const fw = fitWidth.value
    if (!fw) return null
    const width = fw * view.zoomLevel
    const tr =
      isDragging.value || isGesturing.value
        ? 'none'
        : `transform ${ZOOM_TRANSITION}, width ${ZOOM_TRANSITION}, height ${ZOOM_TRANSITION}`
    return {
      width: `${width}px`,
      height: `${(width * fitSize.value.height) / fw}px`,
      transform: `translate(-50%, -50%) translate(${view.zoomOffsetX * view.zoomLevel}px, ${view.zoomOffsetY * view.zoomLevel}px) rotate(${view.rotation}deg)`,
      transition: tr,
    }
  })

  /** Zoom as a percentage of actual image pixels */
  const zoomPercent = computed(() => computeZoomPercent(view.zoomLevel, view.actualZoom))

  const isZoomed = computed(() => view.zoomLevel !== 1)

  /** Last pointer position in client coords; the anchor is re-derived when the layout changes */
  let lastPointer: Point | null = null

  function anchorAt(client: Point | null): Point | null {
    const el = containerRef.value
    if (!el || !client) return null
    const rect = el.getBoundingClientRect()
    return paneAnchor(
      { x: client.x - rect.left, y: client.y - rect.top },
      { width: rect.width, height: rect.height },
      grid.value,
    )
  }

  function anchorFromEvent(e: MouseEvent): Point | null {
    return anchorAt({ x: e.clientX, y: e.clientY })
  }

  function refreshZoomAnchor() {
    view.zoomAnchor = anchorAt(lastPointer) ?? { x: 0, y: 0 }
  }

  watch([grid, containerWidth, containerHeight], refreshZoomAnchor, {
    flush: 'post',
  })

  /** Pane each image sits in, and the fitted image's on-screen box (rotation applied) */
  function panBounds() {
    const pane = paneSize(
      { width: containerWidth.value, height: containerHeight.value },
      grid.value,
    )
    const { width, height } = fitSize.value
    const isSwapped = view.rotation % 180 !== 0
    return { box: isSwapped ? { width: height, height: width } : { width, height }, pane }
  }

  let detentHold: DetentHold | null = null
  let gestureTimer: ReturnType<typeof setTimeout> | undefined

  /** Marks a gesture under way; a pause ends it and a new one starts free of any detent */
  function continueGesture() {
    isGesturing.value = true
    clearTimeout(gestureTimer)
    gestureTimer = setTimeout(() => {
      isGesturing.value = false
      detentHold = null
    }, GESTURE_GAP_MS)
  }

  function panBy(e: WheelEvent) {
    const { box, pane } = panBounds()
    const state = { zoom: view.zoomLevel, offsetX: view.zoomOffsetX, offsetY: view.zoomOffsetY }
    const next = panByScroll(state, { x: e.deltaX, y: e.deltaY }, box, pane)
    view.zoomOffsetX = next.offsetX
    view.zoomOffsetY = next.offsetY
  }

  /** Pinch and mouse wheel zoom at the cursor, settling on fit and 100%; a trackpad scroll pans */
  function handleWheel(e: WheelEvent) {
    e.preventDefault()
    continueGesture()
    const input: WheelInput = e
    if (wheelIntent(input, IS_MAC) === 'scroll') {
      panBy(e)
      return
    }
    const current = view.zoomLevel
    const stops = view.actualZoom === null ? [1] : [1, view.actualZoom]
    const next = current * wheelZoomFactor(input, IS_MAC)
    const settled = settleOnDetents(current, next, stops, detentHold)
    detentHold = settled.hold
    view.setZoom(settled.zoom, anchorFromEvent(e) ?? undefined)
  }

  function handleDoubleClick(e: MouseEvent) {
    view.toggleActualSize(anchorFromEvent(e) ?? undefined)
  }

  function handleMouseDown(e: MouseEvent) {
    if (view.zoomLevel <= 1) return
    isDragging.value = true
    dragStart.value = { x: e.clientX, y: e.clientY }
    offsetStart.value = { x: view.zoomOffsetX, y: view.zoomOffsetY }
    document.body.style.cursor = 'grabbing'
  }

  function handleMouseMove(e: MouseEvent) {
    lastPointer = { x: e.clientX, y: e.clientY }
    refreshZoomAnchor()
    if (!isDragging.value) return
    const dx = (e.clientX - dragStart.value.x) / view.zoomLevel
    const dy = (e.clientY - dragStart.value.y) / view.zoomLevel
    view.zoomOffsetX = offsetStart.value.x + dx
    view.zoomOffsetY = offsetStart.value.y + dy
  }

  function handleMouseUp() {
    isDragging.value = false
    document.body.style.cursor = ''
  }

  /** Zoom to restore once the next photo is measured */
  let pendingCarry: ZoomCarry | null = null

  // Switching photos keeps the zoom (100% stays 100%, same part in view) so a
  // burst can be checked for focus in turn; a rotated view starts over at fit.
  // Index-based so it also fires between photos of the same size.
  watch(
    () => session.currentIndex,
    () => {
      const isRotated = view.rotation % 360 !== 0
      const state = { zoom: view.zoomLevel, offsetX: view.zoomOffsetX, offsetY: view.zoomOffsetY }
      pendingCarry = isRotated ? null : carryZoom(state, view.actualZoom, fitSize.value)
      if (!pendingCarry) view.resetZoom()
    },
  )

  // A photo of another size changes the scale the carried zoom maps to
  watch([naturalWidth, naturalHeight], ([width]) => {
    if (!pendingCarry) return
    const actual = actualSizeZoom(width, fitWidth.value, dpr.value)
    const next = applyZoomCarry(pendingCarry, actual, fitSize.value)
    pendingCarry = null
    view.zoomLevel = next.zoom
    view.zoomOffsetX = next.offsetX
    view.zoomOffsetY = next.offsetY
  })

  return {
    imageStyle,
    overlayStyle,
    fitSize,
    zoomPercent,
    isZoomed,
    isDragging,
    handleWheel,
    handleDoubleClick,
    handleMouseDown,
  }
}
