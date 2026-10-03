import { describe, expect, it } from 'vitest'
import {
  BIN_COUNT,
  computeHistogram,
  formatClipRatio,
  histogramPath,
  isClipWarning,
  plotPeak,
} from './histogram'

/** Build RGBA bytes from [r, g, b] triples */
function rgba(...pixels: [number, number, number][]): Uint8ClampedArray {
  return Uint8ClampedArray.from(pixels.flatMap(([r, g, b]) => [r, g, b, 255]))
}

describe('computeHistogram', () => {
  it('counts each channel into its own bins', () => {
    const h = computeHistogram(rgba([10, 20, 30], [10, 200, 30]))
    expect(h.pixelCount).toBe(2)
    expect(h.red[10]).toBe(2)
    expect(h.green[20]).toBe(1)
    expect(h.green[200]).toBe(1)
    expect(h.blue[30]).toBe(2)
  })

  it('bins luma with Rec. 709 weights', () => {
    const h = computeHistogram(rgba([255, 0, 0], [0, 255, 0], [128, 128, 128]))
    expect(h.luma[54]).toBe(1) // 0.2126 × 255
    expect(h.luma[182]).toBe(1) // 0.7152 × 255
    expect(h.luma[128]).toBe(1)
  })

  it('flags a pixel as blown when any channel reaches the clip level', () => {
    const h = computeHistogram(
      rgba([255, 255, 255], [254, 100, 100], [253, 253, 253], [50, 50, 50]),
    )
    expect(h.highlightClipRatio).toBe(0.5)
  })

  it('flags a pixel as crushed when any channel reaches the shadow level', () => {
    const h = computeHistogram(rgba([0, 0, 0], [1, 80, 80], [2, 2, 2], [50, 50, 50]))
    expect(h.shadowClipRatio).toBe(0.5)
  })

  it('ignores the alpha channel and a trailing partial pixel', () => {
    const h = computeHistogram(Uint8ClampedArray.from([5, 5, 5, 0, 9, 9]))
    expect(h.pixelCount).toBe(1)
    expect(h.red[5]).toBe(1)
  })

  it('reports zero ratios for an empty image', () => {
    const h = computeHistogram(new Uint8ClampedArray(0))
    expect(h.highlightClipRatio).toBe(0)
    expect(h.shadowClipRatio).toBe(0)
  })
})

describe('plotPeak', () => {
  it('ignores the end bins so clipping spikes do not flatten the curve', () => {
    const bins = new Uint32Array(BIN_COUNT)
    bins[0] = 1000
    bins[128] = 40
    bins[255] = 5000
    expect(plotPeak([bins])).toBe(40)
  })

  it('takes the highest bin across channels', () => {
    const a = new Uint32Array(BIN_COUNT)
    const b = new Uint32Array(BIN_COUNT)
    a[10] = 3
    b[20] = 7
    expect(plotPeak([a, b])).toBe(7)
  })
})

describe('histogramPath', () => {
  it('draws a closed area from the baseline, clamping bins above the peak', () => {
    const bins = new Uint32Array(BIN_COUNT)
    bins[0] = 10
    bins[1] = 5
    const path = histogramPath(bins, 5, 100)
    expect(path.startsWith('M0 100 L0 0.0 L1 0.0 L1 0.0 L2 0.0 L2 100.0')).toBe(true)
    expect(path.endsWith(`L${BIN_COUNT} 100 Z`)).toBe(true)
  })

  it('returns an empty path when there is nothing to plot', () => {
    expect(histogramPath(new Uint32Array(BIN_COUNT), 0, 100)).toBe('')
  })
})

describe('clip warning', () => {
  it('warns from 0.1% of pixels', () => {
    expect(isClipWarning(0.001)).toBe(true)
    expect(isClipWarning(0.0009)).toBe(false)
  })

  it('formats ratios for display', () => {
    expect(formatClipRatio(0)).toBe('0.0%')
    expect(formatClipRatio(0.0004)).toBe('<0.1%')
    expect(formatClipRatio(0.0234)).toBe('2.3%')
    expect(formatClipRatio(0.4567)).toBe('46%')
  })
})
