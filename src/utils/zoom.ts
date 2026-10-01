// ============================================================
// Sift - Zoom math
// zoom is relative to the fit size (1 = fit); offsets are in fit-scale CSS px
// and get multiplied by zoom when rendered, so screen = zoom * (offset + p).
// ============================================================

export const MIN_ZOOM = 0.5
export const MAX_ZOOM = 10

export interface Size {
  width: number
  height: number
}

export interface Point {
  x: number
  y: number
}

export interface ZoomState {
  zoom: number
  offsetX: number
  offsetY: number
}

/**
 * Rendered <img> width at zoom = 1: fits the pane, never upscales.
 * Rotation by 90°/270° swaps the bounding box, not the element's own width.
 */
export function fitWidth(natural: Size, pane: Size, rotation: number): number {
  const { width: nw, height: nh } = natural
  if (!nw || !nh || !pane.width || !pane.height) return 0
  const isSwapped = rotation % 180 !== 0
  const boxW = isSwapped ? nh : nw
  const boxH = isSwapped ? nw : nh
  return nw * Math.min(1, pane.width / boxW, pane.height / boxH)
}

/** Zoom level at which one image pixel maps to one device pixel */
export function actualSizeZoom(naturalWidth: number, fit: number, dpr: number): number | null {
  if (!naturalWidth || !fit || !dpr) return null
  return naturalWidth / dpr / fit
}

/**
 * Zoom to `nextZoom` keeping the image point under `anchor` fixed on screen.
 * `anchor` is relative to the pane center. At or below fit, the image recenters.
 */
export function zoomAt(state: ZoomState, nextZoom: number, anchor: Point): ZoomState {
  if (nextZoom <= 1) return { zoom: nextZoom, offsetX: 0, offsetY: 0 }
  const shift = 1 / nextZoom - 1 / state.zoom
  return {
    zoom: nextZoom,
    offsetX: state.offsetX + anchor.x * shift,
    offsetY: state.offsetY + anchor.y * shift,
  }
}

/**
 * Pointer position relative to the center of the pane it is over, or null outside.
 * `pointer` is relative to the viewer's top-left; compare mode splits it into two panes.
 */
export function paneAnchor(pointer: Point, viewer: Size, isCompare: boolean): Point | null {
  const { x, y } = pointer
  if (x < 0 || y < 0 || x > viewer.width || y > viewer.height) return null
  const paneWidth = isCompare ? viewer.width / 2 : viewer.width
  const paneLeft = isCompare && x >= paneWidth ? paneWidth : 0
  return { x: x - paneLeft - paneWidth / 2, y: y - viewer.height / 2 }
}

export function clampZoom(zoom: number, actual: number | null): number {
  const max = Math.max(MAX_ZOOM, actual ?? 0)
  return Math.min(max, Math.max(MIN_ZOOM, zoom))
}

export function isActualSize(zoom: number, actual: number | null): boolean {
  return actual !== null && Math.abs(zoom - actual) < 1e-3
}

/** Zoom as a percentage of actual pixels; falls back to fit-relative before the image loads */
export function zoomPercent(zoom: number, actual: number | null): number {
  return Math.round((actual ? zoom / actual : zoom) * 100)
}
