// ============================================================
// Sift - Histogram and clipping statistics
// Pure functions over RGBA pixel data (as returned by getImageData).
// ============================================================

export const BIN_COUNT = 256
/** A channel at or above this counts as blown (JPEG noise keeps blown areas at 253–255) */
export const HIGHLIGHT_CLIP_LEVEL = 254
/** A channel at or below this counts as crushed */
export const SHADOW_CLIP_LEVEL = 1
/** Clipping below this share of pixels is noise (specular glints, a few dark pixels) */
export const CLIP_WARNING_RATIO = 0.001

export interface Histogram {
  red: Uint32Array
  green: Uint32Array
  blue: Uint32Array
  luma: Uint32Array
  pixelCount: number
  /** Share of pixels with any channel at or above HIGHLIGHT_CLIP_LEVEL */
  highlightClipRatio: number
  /** Share of pixels with any channel at or below SHADOW_CLIP_LEVEL */
  shadowClipRatio: number
}

/** Rec. 709 luma, rounded to a bin index */
function luma(r: number, g: number, b: number): number {
  return Math.round(0.2126 * r + 0.7152 * g + 0.0722 * b)
}

export function computeHistogram(rgba: ArrayLike<number>): Histogram {
  const red = new Uint32Array(BIN_COUNT)
  const green = new Uint32Array(BIN_COUNT)
  const blue = new Uint32Array(BIN_COUNT)
  const lumaBins = new Uint32Array(BIN_COUNT)
  let highlights = 0
  let shadows = 0

  const pixelCount = Math.floor(rgba.length / 4)
  for (let i = 0; i < pixelCount * 4; i += 4) {
    const r = rgba[i]
    const g = rgba[i + 1]
    const b = rgba[i + 2]
    red[r]++
    green[g]++
    blue[b]++
    lumaBins[luma(r, g, b)]++
    if (r >= HIGHLIGHT_CLIP_LEVEL || g >= HIGHLIGHT_CLIP_LEVEL || b >= HIGHLIGHT_CLIP_LEVEL) {
      highlights++
    }
    if (r <= SHADOW_CLIP_LEVEL || g <= SHADOW_CLIP_LEVEL || b <= SHADOW_CLIP_LEVEL) shadows++
  }

  return {
    red,
    green,
    blue,
    luma: lumaBins,
    pixelCount,
    highlightClipRatio: pixelCount ? highlights / pixelCount : 0,
    shadowClipRatio: pixelCount ? shadows / pixelCount : 0,
  }
}

/**
 * Peak used to scale the plot. Bins at the very ends are excluded so a clipping
 * spike (often the tallest bin) does not flatten the rest of the curve.
 */
export function plotPeak(channels: readonly Uint32Array[]): number {
  let peak = 0
  for (const bins of channels) {
    for (let i = 1; i < BIN_COUNT - 1; i++) peak = Math.max(peak, bins[i])
  }
  return peak
}

/** Closed SVG area path in a BIN_COUNT × height box; bins above `peak` are clamped */
export function histogramPath(bins: Uint32Array, peak: number, height: number): string {
  if (!peak) return ''
  const points: string[] = [`M0 ${height}`]
  for (let i = 0; i < BIN_COUNT; i++) {
    const y = height - (Math.min(bins[i], peak) / peak) * height
    points.push(`L${i} ${y.toFixed(1)}`, `L${i + 1} ${y.toFixed(1)}`)
  }
  points.push(`L${BIN_COUNT} ${height}`, 'Z')
  return points.join(' ')
}

export function isClipWarning(ratio: number): boolean {
  return ratio >= CLIP_WARNING_RATIO
}

/** "0.4%" / "12%" / "<0.1%" */
export function formatClipRatio(ratio: number): string {
  const percent = ratio * 100
  if (percent > 0 && percent < 0.1) return '<0.1%'
  return `${percent >= 10 ? Math.round(percent) : percent.toFixed(1)}%`
}
