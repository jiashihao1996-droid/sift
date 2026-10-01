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
  fitWidth as computeFitWidth,
  paneAnchor,
  zoomPercent as computeZoomPercent,
  type Point,
} from '@/utils/zoom'

const ZOOM_TRANSITION = '260ms cubic-bezier(0.22, 1, 0.36, 1)'

export function useImageZoom(
  containerRef: Ref<HTMLElement | null>,
  naturalWidth: Ref<number>,
  naturalHeight: Ref<number>,
) {
  const view = useViewStore()
  const session = useSessionStore()

  const isDragging = ref(false)
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

  /** Fit width of the <img> at zoom=1 (rotation-aware, compare-aware: each pane is half width) */
  const fitWidth = computed(() =>
    computeFitWidth(
      { width: naturalWidth.value, height: naturalHeight.value },
      {
        width: view.compareMode ? containerWidth.value / 2 : containerWidth.value,
        height: containerHeight.value,
      },
      view.rotation,
    ),
  )

  watchEffect(() => {
    view.actualZoom = actualSizeZoom(naturalWidth.value, fitWidth.value, dpr.value)
  })

  /** Explicit width so zoom renders at native resolution; CSS constraints until measured */
  const imageStyle = computed(() => {
    const fw = fitWidth.value
    const rot = view.rotation
    const tr = isDragging.value ? 'none' : `transform ${ZOOM_TRANSITION}, width ${ZOOM_TRANSITION}`

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
      view.compareMode,
    )
  }

  function anchorFromEvent(e: MouseEvent): Point | null {
    return anchorAt({ x: e.clientX, y: e.clientY })
  }

  function refreshZoomAnchor() {
    view.zoomAnchor = anchorAt(lastPointer) ?? { x: 0, y: 0 }
  }

  watch([() => view.compareMode, containerWidth, containerHeight], refreshZoomAnchor, {
    flush: 'post',
  })

  function handleWheel(e: WheelEvent) {
    e.preventDefault()
    const delta = e.deltaY > 0 ? 0.9 : 1.1
    view.setZoom(view.zoomLevel * delta, anchorFromEvent(e) ?? undefined)
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

  // Reset zoom/rotation when switching images (index-based to handle same-size photos)
  watch(
    () => session.currentIndex,
    () => {
      view.resetZoom()
    },
  )

  return {
    imageStyle,
    zoomPercent,
    isZoomed,
    isDragging,
    handleWheel,
    handleDoubleClick,
    handleMouseDown,
  }
}
