// ============================================================
// Sift - Focus point geometry
// The camera records where it focused in pixels of the stored image,
// before EXIF Orientation; the viewer shows the photo upright. These
// rules map those pixels onto the upright photo as shares of its width
// and height, and find the zoom offset that centres the focus.
// Pure — no Vue, Pinia or Tauri imports.
// ============================================================

import type { AfInfo } from '@/types'
import type { Point, Size } from './zoom'

/** A position as shares (0–1) of the upright photo's width and height */
export interface SharePoint {
  x: number
  y: number
}

/** A box centred on (x, y), all as shares of the upright photo */
export interface ShareBox extends SharePoint {
  width: number
  height: number
}

export interface AfMarks {
  /** What the camera focused on (an eye, a face, an AF point) */
  area: ShareBox | null
  /** Phase-detect points that were in focus */
  points: SharePoint[]
}

/** EXIF orientations 5–8 turn the image a quarter, swapping width and height */
function isQuarterTurn(orientation: number): boolean {
  return orientation >= 5 && orientation <= 8
}

/**
 * Where the stored image's (u, v) — shares of its width and height — ends
 * up once EXIF `orientation` is applied. Unknown values count as 1 (as is).
 */
export function orientShare(u: number, v: number, orientation: number): SharePoint {
  switch (orientation) {
    case 2: // mirrored left-right
      return { x: 1 - u, y: v }
    case 3: // rotated 180°
      return { x: 1 - u, y: 1 - v }
    case 4: // mirrored top-bottom
      return { x: u, y: 1 - v }
    case 5: // transposed
      return { x: v, y: u }
    case 6: // rotated 90° clockwise
      return { x: 1 - v, y: u }
    case 7: // transversed
      return { x: 1 - v, y: 1 - u }
    case 8: // rotated 90° counter-clockwise
      return { x: v, y: 1 - u }
    default:
      return { x: u, y: v }
  }
}

/** Focus marks on the upright photo; null when the camera recorded none */
export function afMarks(info: AfInfo | undefined): AfMarks | null {
  if (info?.kind !== 'found' || !info.width || !info.height) return null
  const { width, height, orientation } = info
  const toShare = (x: number, y: number) => orientShare(x / width, y / height, orientation)
  const isTurned = isQuarterTurn(orientation)
  const area = info.area
    ? {
        ...toShare(info.area.x, info.area.y),
        width: (isTurned ? info.area.height : info.area.width) / (isTurned ? height : width),
        height: (isTurned ? info.area.width : info.area.height) / (isTurned ? width : height),
      }
    : null
  return { area, points: info.points.map((p) => toShare(p.x, p.y)) }
}

/** Width ÷ height of the upright photo the focus data describes; null without any */
export function uprightAspect(info: AfInfo | undefined): number | null {
  if (info?.kind !== 'found' || !info.width || !info.height) return null
  const ratio = info.width / info.height
  return isQuarterTurn(info.orientation) ? 1 / ratio : ratio
}

/** Where to centre on: the focus area, else the middle of the in-focus points */
export function focusCenter(marks: AfMarks | null): SharePoint | null {
  if (!marks) return null
  if (marks.area) return { x: marks.area.x, y: marks.area.y }
  const { points } = marks
  if (points.length === 0) return null
  const sum = points.reduce((acc, p) => ({ x: acc.x + p.x, y: acc.y + p.y }), { x: 0, y: 0 })
  return { x: sum.x / points.length, y: sum.y / points.length }
}

/**
 * Zoom offset (fit-scale px, see core/zoom.ts) that puts `point` of the
 * upright photo at the pane centre. `fit` is the photo's fitted size before
 * the viewer's own `rotation` (degrees clockwise), which turns the point
 * about the photo's centre.
 */
export function centerOffset(point: SharePoint, fit: Size, rotation: number): Point {
  const x = (point.x - 0.5) * fit.width
  const y = (point.y - 0.5) * fit.height
  const radians = (rotation * Math.PI) / 180
  const cos = Math.round(Math.cos(radians) * 1e9) / 1e9
  const sin = Math.round(Math.sin(radians) * 1e9) / 1e9
  // Negated so the point moves to the centre; `+ 0` keeps -0 out of the result
  return { x: -(x * cos - y * sin) + 0, y: -(x * sin + y * cos) + 0 }
}

/** Why there is nothing to draw, for photos without focus marks */
export function afHint(info: AfInfo | undefined): string | null {
  switch (info?.kind) {
    case 'manualFocus':
      return '手动对焦，没有对焦点'
    case 'unavailable':
      return '此照片没有对焦点信息'
    default:
      return null
  }
}
