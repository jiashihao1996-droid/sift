import { describe, expect, it } from 'vitest'
import { layoutGrid, visibleTiles } from './gridLayout'

const METRICS = { minItemWidth: 110, gap: 8, aspect: 3 / 4 }

describe('layoutGrid', () => {
  it('fits as many columns as auto-fill would and stretches them to fill the width', () => {
    // 6 columns need 6 * 110 + 5 * 8 = 700; a seventh would need 818
    const layout = layoutGrid(100, 760, METRICS)
    expect(layout.columns).toBe(6)
    const itemWidth = (760 - 5 * 8) / 6
    expect(layout.rowHeight).toBeCloseTo(itemWidth * 0.75)
    expect(layout.rowStride).toBeCloseTo(itemWidth * 0.75 + 8)
  })

  it('adds a column exactly when it fits at the minimum width', () => {
    expect(layoutGrid(10, 817, METRICS).columns).toBe(6)
    expect(layoutGrid(10, 818, METRICS).columns).toBe(7)
  })

  it('keeps one column when the container is narrower than a tile', () => {
    const layout = layoutGrid(3, 80, METRICS)
    expect(layout.columns).toBe(1)
    expect(layout.rowHeight).toBe(60)
    expect(layout.rows).toBe(3)
  })

  it('counts a part-filled last row and leaves no gap below it', () => {
    // 4 columns of 100 at width 424
    const layout = layoutGrid(9, 424, { ...METRICS, minItemWidth: 100 })
    expect(layout.columns).toBe(4)
    expect(layout.rows).toBe(3)
    expect(layout.height).toBe(3 * 75 + 2 * 8)
  })

  it('is empty with no tiles', () => {
    const layout = layoutGrid(0, 760, METRICS)
    expect(layout.rows).toBe(0)
    expect(layout.height).toBe(0)
  })
})

describe('visibleTiles', () => {
  // 4 columns of 100 → rows 75 high, a row every 83
  const layout = layoutGrid(20_000, 424, { ...METRICS, minItemWidth: 100 })

  it('covers the rows in view plus the overscan', () => {
    // 0..300 shows rows 0..3 (row 3 starts at 249)
    expect(visibleTiles(layout, 20_000, 0, 300, 0)).toEqual({ start: 0, end: 16, top: 0 })
    expect(visibleTiles(layout, 20_000, 0, 300, 2)).toEqual({ start: 0, end: 24, top: 0 })
  })

  it('follows the scroll position and places the first rendered row', () => {
    const scrollTop = 1000 * 83 + 10
    const { start, end, top } = visibleTiles(layout, 20_000, scrollTop, 300, 2)
    expect(start).toBe(998 * 4)
    expect(top).toBe(998 * 83)
    // Rows 1000..1003 in view, two more below
    expect(end).toBe(1006 * 4)
  })

  it('stops at the last tile of a part-filled row', () => {
    const small = layoutGrid(9, 424, { ...METRICS, minItemWidth: 100 })
    expect(visibleTiles(small, 9, 0, 1000, 3)).toEqual({ start: 0, end: 9, top: 0 })
  })

  it('shows the last rows when the scroll position is past the end', () => {
    // The category switched to a shorter one before the scroll position caught up
    const small = layoutGrid(40, 424, { ...METRICS, minItemWidth: 100 })
    const { start, end } = visibleTiles(small, 40, 50_000, 300, 0)
    expect(end).toBe(40)
    expect(start).toBeLessThan(end)
  })

  it('treats a negative scroll position as the top', () => {
    expect(visibleTiles(layout, 20_000, -16, 300, 0).start).toBe(0)
  })

  it('renders nothing with no tiles', () => {
    const empty = layoutGrid(0, 424, METRICS)
    expect(visibleTiles(empty, 0, 0, 300, 2)).toEqual({ start: 0, end: 0, top: 0 })
  })
})
