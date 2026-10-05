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
  paneAnchor,
  SINGLE_PANE,
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
