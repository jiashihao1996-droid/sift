import { describe, expect, it } from 'vitest'
import { afHint, afMarks, centerOffset, focusCenter, orientShare, uprightAspect } from './afPoint'
import type { AfInfo } from '@/types'

function found(overrides: Partial<Extract<AfInfo, { kind: 'found' }>> = {}): AfInfo {
  return {
    kind: 'found',
    width: 6000,
    height: 4000,
    orientation: 1,
    area: { x: 1500, y: 1000, width: 600, height: 200 },
    points: [],
    ...overrides,
  }
}

describe('orientShare', () => {
  // The stored image's top-left corner, and where each orientation shows it
  it.each([
    [1, { x: 0, y: 0 }],
    [2, { x: 1, y: 0 }],
    [3, { x: 1, y: 1 }],
    [4, { x: 0, y: 1 }],
    [5, { x: 0, y: 0 }],
    [6, { x: 1, y: 0 }],
    [7, { x: 1, y: 1 }],
    [8, { x: 0, y: 1 }],
  ])('moves the top-left corner for orientation %i', (orientation, expected) => {
    expect(orientShare(0, 0, orientation)).toEqual(expected)
  })

  it('turns a point a quarter clockwise for orientation 6', () => {
    // A quarter of the way across the top edge ends up a quarter of the way down the right edge
    expect(orientShare(0.25, 0, 6)).toEqual({ x: 1, y: 0.25 })
  })

  it('treats an unknown orientation as upright', () => {
    expect(orientShare(0.3, 0.6, 0)).toEqual({ x: 0.3, y: 0.6 })
  })
})

describe('afMarks', () => {
  it('maps the focus area to shares of an upright photo', () => {
    expect(afMarks(found())?.area).toEqual({ x: 0.25, y: 0.25, width: 0.1, height: 0.05 })
  })

  it('swaps the area width and height for a portrait photo stored sideways', () => {
    const area = afMarks(found({ orientation: 8 }))?.area
    expect(area).toEqual({ x: 0.25, y: 0.75, width: 0.05, height: 0.1 })
  })

  it('maps the in-focus points too', () => {
    const marks = afMarks(found({ area: null, points: [{ x: 3000, y: 2000 }] }))
    expect(marks).toEqual({ area: null, points: [{ x: 0.5, y: 0.5 }] })
  })

  it('is null without focus data', () => {
    expect(afMarks(undefined)).toBeNull()
    expect(afMarks({ kind: 'manualFocus' })).toBeNull()
    expect(afMarks({ kind: 'unavailable' })).toBeNull()
    expect(afMarks(found({ width: 0 }))).toBeNull()
  })
})

describe('uprightAspect', () => {
  it('is width over height of the photo as shown', () => {
    expect(uprightAspect(found())).toBe(1.5)
    expect(uprightAspect(found({ orientation: 6 }))).toBeCloseTo(2 / 3)
  })

  it('is null without focus data', () => {
    expect(uprightAspect({ kind: 'unavailable' })).toBeNull()
    expect(uprightAspect(found({ height: 0 }))).toBeNull()
  })
})

describe('focusCenter', () => {
  it('prefers the focus area', () => {
    expect(focusCenter(afMarks(found({ points: [{ x: 0, y: 0 }] })))).toEqual({
      x: 0.25,
      y: 0.25,
    })
  })

  it('falls back to the middle of the in-focus points', () => {
    const marks = afMarks(
      found({
        area: null,
        points: [
          { x: 0, y: 0 },
          { x: 3000, y: 2000 },
        ],
      }),
    )
    expect(focusCenter(marks)).toEqual({ x: 0.25, y: 0.25 })
  })

  it('is null with nothing to centre on', () => {
    expect(focusCenter(null)).toBeNull()
    expect(focusCenter({ area: null, points: [] })).toBeNull()
  })
})

describe('centerOffset', () => {
  const fit = { width: 800, height: 600 }

  it('moves the point to the pane centre', () => {
    expect(centerOffset({ x: 0.25, y: 0.75 }, fit, 0)).toEqual({ x: 200, y: -150 })
  })

  it('is zero for the photo centre', () => {
    expect(centerOffset({ x: 0.5, y: 0.5 }, fit, 90)).toEqual({ x: 0, y: 0 })
  })

  it('follows the viewer rotation', () => {
    // The top-left quarter point sits at (-200, -150) from the centre; turned
    // 90° clockwise it is at (150, -200), so the image moves by the opposite
    expect(centerOffset({ x: 0.25, y: 0.25 }, fit, 90)).toEqual({ x: -150, y: 200 })
    expect(centerOffset({ x: 0.25, y: 0.25 }, fit, 180)).toEqual({ x: -200, y: -150 })
    expect(centerOffset({ x: 0.25, y: 0.25 }, fit, 450)).toEqual({ x: -150, y: 200 })
  })
})

describe('afHint', () => {
  it('explains manual focus and missing data', () => {
    expect(afHint({ kind: 'manualFocus' })).toBe('手动对焦，没有对焦点')
    expect(afHint({ kind: 'unavailable' })).toBe('此照片没有对焦点信息')
  })

  it('says nothing while loading or when there is something to draw', () => {
    expect(afHint(undefined)).toBeNull()
    expect(afHint(found())).toBeNull()
  })
})
