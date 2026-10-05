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

/** How the viewer is split into equal panes: compare is 2×1, a survey up to 2×2 */
export interface PaneGrid {
  cols: number
  rows: number
}

export const SINGLE_PANE: PaneGrid = { cols: 1, rows: 1 }

/** Size of one pane of the grid */
export function paneSize(viewer: Size, grid: PaneGrid): Size {
  return { width: viewer.width / grid.cols, height: viewer.height / grid.rows }
}

/**
 * Pointer position relative to the center of the pane it is over, or null outside.
 * `pointer` is relative to the viewer's top-left.
 */
export function paneAnchor(pointer: Point, viewer: Size, grid: PaneGrid): Point | null {
  const { x, y } = pointer
  if (x < 0 || y < 0 || x > viewer.width || y > viewer.height) return null
  const pane = paneSize(viewer, grid)
  const col = Math.min(grid.cols - 1, Math.floor(x / pane.width))
  const row = Math.min(grid.rows - 1, Math.floor(y / pane.height))
  return {
    x: x - col * pane.width - pane.width / 2,
    y: y - row * pane.height - pane.height / 2,
  }
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

/**
 * Zoom kept across a photo switch, independent of the photo's size: zoom as a
 * multiple of actual pixels (so 100% stays 100%) and the offset as a share of
 * the fitted image (so the same part of a burst stays in view).
 */
export interface ZoomCarry {
  ofActual: number
  shareX: number
  shareY: number
}

const FIT: ZoomState = { zoom: 1, offsetX: 0, offsetY: 0 }

/** What to carry to the next photo; null at fit or before the photo is measured */
export function carryZoom(state: ZoomState, actual: number | null, fit: Size): ZoomCarry | null {
  if (state.zoom <= 1 || !actual || !fit.width || !fit.height) return null
  return {
    ofActual: state.zoom / actual,
    shareX: state.offsetX / fit.width,
    shareY: state.offsetY / fit.height,
  }
}

/** Zoom state for the next photo, given its actual-size zoom and fitted size */
export function applyZoomCarry(carry: ZoomCarry, actual: number | null, fit: Size): ZoomState {
  if (!actual) return { ...FIT }
  const zoom = clampZoom(carry.ofActual * actual, actual)
  if (zoom <= 1) return { ...FIT, zoom }
  return { zoom, offsetX: carry.shareX * fit.width, offsetY: carry.shareY * fit.height }
}

/**
 * How far past a detent a gesture has to push, in natural-log zoom, before it
 * moves on: about 10%, so a pinch or wheel settles on fit and 100% on its way.
 */
export const DETENT_HOLD = Math.log(1.1)

/** A gesture held at a detent and how far it has pushed past it (log zoom, signed) */
export interface DetentHold {
  stop: number
  push: number
}

export interface DetentResult {
  zoom: number
  hold: DetentHold | null
}

/** First stop a step from `current` to `next` crosses (or lands on), if any */
function firstCrossed(current: number, next: number, stops: readonly number[]): number | undefined {
  const isZoomingIn = next > current
  return stops
    .filter((s) => (isZoomingIn ? s > current && s <= next : s < current && s >= next))
    .sort((a, b) => (isZoomingIn ? a - b : b - a))[0]
}

/** At `stop`, pushed `push` past it: hold there, or move on and look for the next stop */
function passStop(stop: number, push: number, stops: readonly number[]): DetentResult {
  if (Math.abs(push) <= DETENT_HOLD) return { zoom: stop, hold: { stop, push } }
  const rest = stops.filter((s) => s !== stop)
  return settleOnDetents(stop, stop * Math.exp(push - Math.sign(push) * DETENT_HOLD), rest, null)
}

/**
 * Zoom for one step of a continuous gesture from `current` toward `next`,
 * stopping at each of `stops` (fit, 100%) it crosses until it pushes
 * DETENT_HOLD further. `hold` carries over between steps of one gesture;
 * pass null when a new gesture starts.
 */
export function settleOnDetents(
  current: number,
  next: number,
  stops: readonly number[],
  hold: DetentHold | null,
): DetentResult {
  if (hold) return passStop(hold.stop, hold.push + Math.log(next / current), stops)
  const stop = firstCrossed(current, next, stops)
  if (stop === undefined) return { zoom: next, hold: null }
  return passStop(stop, Math.log(next / stop), stops)
}

/** Farthest offset that keeps the image covering the pane, per axis (fit-scale px) */
function panLimit(box: number, pane: number, zoom: number): number {
  return Math.max(0, (box * zoom - pane) / 2) / zoom
}

function panAxis(offset: number, move: number, limit: number): number {
  const next = offset + move
  if (Math.abs(next) <= limit) return next
  // Already outside on this side (dragged there): don't push further out, but don't snap back
  const bound = Math.sign(next) === Math.sign(offset) ? Math.max(limit, Math.abs(offset)) : limit
  return Math.sign(next) * Math.min(Math.abs(next), bound)
}

/**
 * Pan by a scroll of (dx, dy) screen px: the image moves against the scroll,
 * like a page, and stops where its edge meets the pane's. `box` is the fitted
 * image's on-screen box (rotation applied). Nothing to pan at or below fit.
 */
export function panByScroll(state: ZoomState, scroll: Point, box: Size, pane: Size): ZoomState {
  const { zoom } = state
  if (zoom <= 1) return state
  return {
    zoom,
    offsetX: panAxis(state.offsetX, -scroll.x / zoom, panLimit(box.width, pane.width, zoom)),
    offsetY: panAxis(state.offsetY, -scroll.y / zoom, panLimit(box.height, pane.height, zoom)),
  }
}
