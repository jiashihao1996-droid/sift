// ============================================================
// Sift - Thumbnail strip layout
// Where each thumbnail of the bottom strip sits, so the strip can render
// only the ones in view: a 20,000-photo folder would otherwise be 20,000
// elements re-rendered on every key press.
// Pure — no Vue, Pinia or Tauri imports.
// ============================================================

export interface StripMetrics {
  /** Width of one thumbnail */
  itemWidth: number
  /** Space between neighbouring thumbnails */
  gap: number
  /** Extra space where a burst starts or ends */
  burstGap: number
  /** Space before the first and after the last thumbnail */
  padding: number
}

export interface StripLayout {
  /** Left edge of each thumbnail, from the strip's start */
  lefts: Float64Array
  /** Width of the whole strip, padding included */
  width: number
}

/** Positions of `count` thumbnails; `isSpacedBefore(i)` adds the burst gap before i */
export function layoutStrip(
  count: number,
  isSpacedBefore: (index: number) => boolean,
  { itemWidth, gap, burstGap, padding }: StripMetrics,
): StripLayout {
  const lefts = new Float64Array(count)
  let left = padding
  for (let i = 0; i < count; i++) {
    if (i > 0) left += itemWidth + gap
    if (isSpacedBefore(i)) left += burstGap
    lefts[i] = left
  }
  const width = count > 0 ? left + itemWidth + padding : 2 * padding
  return { lefts, width }
}

/** First thumbnail whose right edge is past `x` (lefts are ascending) */
function firstEndingAfter(lefts: Float64Array, x: number, itemWidth: number): number {
  let lo = 0
  let hi = lefts.length
  while (lo < hi) {
    const mid = (lo + hi) >>> 1
    if (lefts[mid] + itemWidth <= x) lo = mid + 1
    else hi = mid
  }
  return lo
}

/**
 * Thumbnails to render for a viewport `scrollLeft`..`scrollLeft + viewport`:
 * `start` inclusive, `end` exclusive, widened by `overscan` on each side
 */
export function visibleRange(
  layout: StripLayout,
  scrollLeft: number,
  viewport: number,
  itemWidth: number,
  overscan: number,
): { start: number; end: number } {
  const count = layout.lefts.length
  const first = firstEndingAfter(layout.lefts, scrollLeft, itemWidth)
  const last = firstEndingAfter(layout.lefts, scrollLeft + viewport, itemWidth)
  return {
    start: Math.max(0, first - overscan),
    end: Math.min(count, last + 1 + overscan),
  }
}

/** Scroll position that centres thumbnail `index`, within the strip's bounds */
export function scrollToCenter(
  layout: StripLayout,
  index: number,
  viewport: number,
  itemWidth: number,
): number {
  const left = layout.lefts[index]
  if (left === undefined) return 0
  const target = left + itemWidth / 2 - viewport / 2
  return Math.min(Math.max(0, target), Math.max(0, layout.width - viewport))
}
