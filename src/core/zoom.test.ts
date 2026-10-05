import { describe, expect, it } from 'vitest'
import {
  actualSizeZoom,
  applyZoomCarry,
  carryZoom,
  clampZoom,
  fitWidth,
  isActualSize,
  MAX_ZOOM,
  MIN_ZOOM,
  DETENT_HOLD,
  panByScroll,
  paneAnchor,
  SINGLE_PANE,
  settleOnDetents,
  zoomAt,
  zoomPercent,
} from './zoom'

describe('fitWidth', () => {
  it('fits a landscape image to the pane width', () => {
    expect(fitWidth({ width: 6000, height: 4000 }, { width: 1200, height: 1000 }, 0)).toBe(1200)
  })

  it('fits a portrait image to the pane height', () => {
    expect(fitWidth({ width: 4000, height: 6000 }, { width: 1200, height: 900 }, 0)).toBe(600)
  })

  it('uses the swapped bounding box when rotated 90°', () => {
    // Bounding box is 4000x6000 → height-limited to 900 → scale 0.15
    expect(fitWidth({ width: 6000, height: 4000 }, { width: 1200, height: 900 }, 90)).toBe(900)
  })

  it('never upscales an image smaller than the pane', () => {
    expect(fitWidth({ width: 800, height: 600 }, { width: 1200, height: 900 }, 0)).toBe(800)
  })

  it('returns 0 until sizes are known', () => {
    expect(fitWidth({ width: 0, height: 0 }, { width: 1200, height: 900 }, 0)).toBe(0)
    expect(fitWidth({ width: 6000, height: 4000 }, { width: 0, height: 0 }, 0)).toBe(0)
  })
})

describe('actualSizeZoom', () => {
  it('maps one image pixel to one device pixel on a 2x display', () => {
    expect(actualSizeZoom(6000, 1200, 2)).toBe(2.5)
  })

  it('returns null before the fit width is measured', () => {
    expect(actualSizeZoom(6000, 0, 2)).toBeNull()
  })
})

describe('zoomAt', () => {
  it('keeps the anchored image point under the cursor', () => {
    const before = { zoom: 1.5, offsetX: 20, offsetY: -10 }
    const anchor = { x: 300, y: -120 }
    const after = zoomAt(before, 4, anchor)

    // The fit-space point under the cursor: anchor / zoom - offset
    const pointBefore = {
      x: anchor.x / before.zoom - before.offsetX,
      y: anchor.y / before.zoom - before.offsetY,
    }
    const pointAfter = {
      x: anchor.x / after.zoom - after.offsetX,
      y: anchor.y / after.zoom - after.offsetY,
    }
    expect(pointAfter.x).toBeCloseTo(pointBefore.x)
    expect(pointAfter.y).toBeCloseTo(pointBefore.y)
  })

  it('zooms around the center when the anchor is the center', () => {
    expect(zoomAt({ zoom: 1, offsetX: 0, offsetY: 0 }, 3, { x: 0, y: 0 })).toEqual({
      zoom: 3,
      offsetX: 0,
      offsetY: 0,
    })
  })

  it('recenters the image at or below fit', () => {
    expect(zoomAt({ zoom: 3, offsetX: 50, offsetY: 40 }, 1, { x: 100, y: 100 })).toEqual({
      zoom: 1,
      offsetX: 0,
      offsetY: 0,
    })
  })
})

describe('clampZoom', () => {
  it('clamps to the fixed range', () => {
    expect(clampZoom(0.1, 2)).toBe(MIN_ZOOM)
    expect(clampZoom(50, 2)).toBe(MAX_ZOOM)
  })

  it('allows reaching actual size beyond the fixed maximum', () => {
    expect(clampZoom(12, 12)).toBe(12)
  })
})

describe('isActualSize', () => {
  it('matches within floating point noise', () => {
    expect(isActualSize(2.5000001, 2.5)).toBe(true)
    expect(isActualSize(2, 2.5)).toBe(false)
    expect(isActualSize(1, null)).toBe(false)
  })
})

describe('zoomPercent', () => {
  it('reports percentage of actual pixels', () => {
    expect(zoomPercent(2.5, 2.5)).toBe(100)
    expect(zoomPercent(1, 2.5)).toBe(40)
  })

  it('falls back to fit-relative percentage', () => {
    expect(zoomPercent(2, null)).toBe(200)
  })
})

describe('paneAnchor', () => {
  const viewer = { width: 1200, height: 800 }
  const COMPARE = { cols: 2, rows: 1 }

  it('is relative to the center of the quarter it is over in a 2×2 survey', () => {
    expect(paneAnchor({ x: 700, y: 500 }, viewer, { cols: 2, rows: 2 })).toEqual({
      x: -200,
      y: -100,
    })
  })

  it('is relative to the viewer center in normal mode', () => {
    expect(paneAnchor({ x: 900, y: 100 }, viewer, SINGLE_PANE)).toEqual({ x: 300, y: -300 })
  })

  it('is relative to the left pane center in compare mode', () => {
    expect(paneAnchor({ x: 100, y: 400 }, viewer, COMPARE)).toEqual({ x: -200, y: 0 })
  })

  it('is relative to the right pane center in compare mode', () => {
    expect(paneAnchor({ x: 1000, y: 400 }, viewer, COMPARE)).toEqual({ x: 100, y: 0 })
  })

  it('returns null outside the viewer', () => {
    expect(paneAnchor({ x: -1, y: 400 }, viewer, SINGLE_PANE)).toBeNull()
    expect(paneAnchor({ x: 600, y: 801 }, viewer, SINGLE_PANE)).toBeNull()
  })
})

describe('carryZoom / applyZoomCarry', () => {
  const fit = { width: 1500, height: 1000 }

  it('keeps zoom and position unchanged between photos of the same size', () => {
    const state = { zoom: 2, offsetX: -300, offsetY: 120 }

    const carry = carryZoom(state, 2, fit)

    expect(applyZoomCarry(carry!, 2, fit)).toEqual(state)
  })

  it('keeps 100% at 100% on a photo with a different resolution', () => {
    const carry = carryZoom({ zoom: 2, offsetX: 0, offsetY: 0 }, 2, fit)

    const next = applyZoomCarry(carry!, 4 / 3, fit)

    expect(isActualSize(next.zoom, 4 / 3)).toBe(true)
  })

  it('keeps the position as a share of the image', () => {
    const carry = carryZoom({ zoom: 2, offsetX: -300, offsetY: 100 }, 2, fit)

    const next = applyZoomCarry(carry!, 2, { width: 1200, height: 800 })

    expect([next.offsetX, next.offsetY]).toEqual([-240, 80])
  })

  it('carries nothing at fit or before the photo is measured', () => {
    expect(carryZoom({ zoom: 1, offsetX: 0, offsetY: 0 }, 2, fit)).toBeNull()
    expect(carryZoom({ zoom: 2, offsetX: 0, offsetY: 0 }, null, fit)).toBeNull()
    expect(carryZoom({ zoom: 2, offsetX: 0, offsetY: 0 }, 2, { width: 0, height: 0 })).toBeNull()
  })

  it('falls back to fit when the next photo cannot be measured', () => {
    const carry = carryZoom({ zoom: 2, offsetX: -300, offsetY: 0 }, 2, fit)

    expect(applyZoomCarry(carry!, null, fit)).toEqual({ zoom: 1, offsetX: 0, offsetY: 0 })
  })
})

describe('settleOnDetents', () => {
  const STOPS = [1, 2]

  it('passes through when no stop lies between the two zooms', () => {
    expect(settleOnDetents(1.2, 1.5, STOPS, null)).toEqual({ zoom: 1.5, hold: null })
  })

  it('stops at fit when zooming out across it', () => {
    const result = settleOnDetents(1.05, 0.98, STOPS, null)
    expect(result.zoom).toBe(1)
    expect(result.hold?.stop).toBe(1)
  })

  it('stops at 100% when zooming in across it', () => {
    expect(settleOnDetents(1.9, 2.1, STOPS, null).zoom).toBe(2)
  })

  it('stops at the next stop when one step jumps past the first', () => {
    // Far past the first stop, but within DETENT_HOLD of the second once released
    expect(settleOnDetents(0.95, 2.4, STOPS, null).zoom).toBe(2)
    expect(settleOnDetents(2.2, 0.85, STOPS, null).zoom).toBe(1)
  })

  it('holds until the gesture pushes DETENT_HOLD past the stop, then moves on smoothly', () => {
    let step = settleOnDetents(1.9, 2.05, STOPS, null)
    let zoom = step.zoom
    const seen: number[] = []
    for (let i = 0; i < 20; i++) {
      const next = zoom * 1.02
      step = settleOnDetents(zoom, next, STOPS, step.hold)
      zoom = step.zoom
      seen.push(zoom)
    }
    const held = seen.filter((z) => z === 2).length
    // ~2.5% pushed by the first step, then 2% per step until 10% in all
    expect(held).toBeGreaterThanOrEqual(3)
    expect(held).toBeLessThanOrEqual(5)
    const firstFree = seen.find((z) => z !== 2) ?? 0
    expect(firstFree).toBeGreaterThan(2)
    expect(firstFree).toBeLessThan(2 * 1.03)
  })

  it('lets a held gesture back out the way it came', () => {
    const held = settleOnDetents(1.9, 2.05, STOPS, null)
    const back = settleOnDetents(2, 2 / 1.2, STOPS, held.hold)
    expect(back.hold).toBeNull()
    expect(back.zoom).toBeLessThan(2)
  })

  it('jumps a stop when a single step pushes far past it', () => {
    const result = settleOnDetents(0.9, 1.5, STOPS, null)
    expect(result.hold).toBeNull()
    expect(result.zoom).toBeCloseTo(1.5 / Math.exp(DETENT_HOLD), 10)
  })

  it('moves freely away from a stop it starts on', () => {
    expect(settleOnDetents(1, 1.05, STOPS, null)).toEqual({ zoom: 1.05, hold: null })
  })
})

describe('panByScroll', () => {
  const BOX = { width: 1000, height: 600 }
  const PANE = { width: 1000, height: 800 }

  it('moves the image against the scroll, like a page', () => {
    const next = panByScroll({ zoom: 2, offsetX: 0, offsetY: 0 }, { x: 10, y: 20 }, BOX, PANE)
    expect(next).toEqual({ zoom: 2, offsetX: -5, offsetY: -10 })
  })

  it('stops where the image edge meets the pane edge', () => {
    // At 2x the image is 2000x1200 on screen: 500 px spare each side horizontally, 200 vertically
    const state = { zoom: 2, offsetX: 0, offsetY: 0 }
    const next = panByScroll(state, { x: 5000, y: -5000 }, BOX, PANE)
    expect(next.offsetX).toBe(-250)
    expect(next.offsetY).toBe(100)
  })

  it('does not pan an axis where the image is smaller than the pane', () => {
    const next = panByScroll({ zoom: 1.2, offsetX: 0, offsetY: 0 }, { x: 0, y: 50 }, BOX, PANE)
    expect(next.offsetY).toBeCloseTo(0, 10)
  })

  it('leaves an image dragged past the edge where it is instead of snapping it back', () => {
    const state = { zoom: 2, offsetX: -400, offsetY: 0 }
    expect(panByScroll(state, { x: 10, y: 0 }, BOX, PANE).offsetX).toBe(-400)
    expect(panByScroll(state, { x: -10, y: 0 }, BOX, PANE).offsetX).toBe(-395)
  })

  it('clamps to the far edge when a scroll carries it past the center', () => {
    const state = { zoom: 2, offsetX: -400, offsetY: 0 }
    expect(panByScroll(state, { x: -2000, y: 0 }, BOX, PANE).offsetX).toBe(250)
  })

  it('does nothing at or below fit', () => {
    const state = { zoom: 1, offsetX: 0, offsetY: 0 }
    expect(panByScroll(state, { x: 30, y: 30 }, BOX, PANE)).toBe(state)
  })
})
