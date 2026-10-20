import { describe, expect, it } from 'vitest'
import { layoutStrip, scrollToCenter, visibleRange } from './stripLayout'

const METRICS = { itemWidth: 64, gap: 6, burstGap: 10, padding: 64 }
const W = METRICS.itemWidth

function plain(count: number) {
  return layoutStrip(count, () => false, METRICS)
}

describe('layoutStrip', () => {
  it('lays thumbnails out one gap apart after the padding', () => {
    const { lefts, width } = plain(3)
    expect([...lefts]).toEqual([64, 134, 204])
    expect(width).toBe(204 + 64 + 64)
  })

  it('adds the burst gap where a burst starts or ends', () => {
    const { lefts } = layoutStrip(4, (i) => i === 2, METRICS)
    expect([...lefts]).toEqual([64, 134, 214, 284])
  })

  it('is just the padding when empty', () => {
    expect(plain(0).width).toBe(128)
  })
})

describe('visibleRange', () => {
  it('covers the thumbnails in view plus the overscan', () => {
    const layout = plain(1000)
    // Viewport 0..700 shows thumbnails 0..9 (the 10th starts at 694)
    expect(visibleRange(layout, 0, 700, W, 0)).toEqual({ start: 0, end: 10 })
    expect(visibleRange(layout, 0, 700, W, 5)).toEqual({ start: 0, end: 15 })
  })

  it('follows the scroll position', () => {
    const layout = plain(1000)
    const at = 64 + 500 * 70
    const { start, end } = visibleRange(layout, at, 700, W, 2)
    expect(start).toBe(498)
    expect(end).toBe(513)
  })

  it('stays within the strip at its end', () => {
    const layout = plain(20)
    expect(visibleRange(layout, layout.width - 300, 300, W, 10).end).toBe(20)
  })

  it('handles an empty strip', () => {
    expect(visibleRange(plain(0), 0, 700, W, 5)).toEqual({ start: 0, end: 0 })
  })
})

describe('scrollToCenter', () => {
  it('centres a thumbnail in the viewport', () => {
    const layout = plain(1000)
    expect(scrollToCenter(layout, 500, 700, W)).toBe(64 + 500 * 70 + 32 - 350)
  })

  it('stops at the strip edges', () => {
    const layout = plain(1000)
    expect(scrollToCenter(layout, 0, 700, W)).toBe(0)
    expect(scrollToCenter(layout, 999, 700, W)).toBe(layout.width - 700)
  })

  it('is 0 for a missing thumbnail or a strip narrower than the viewport', () => {
    expect(scrollToCenter(plain(3), 7, 700, W)).toBe(0)
    expect(scrollToCenter(plain(3), 2, 700, W)).toBe(0)
  })
})
