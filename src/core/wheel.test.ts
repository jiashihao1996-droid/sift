import { describe, expect, it } from 'vitest'
import {
  PINCH_GAIN,
  WHEEL_NOTCH_ZOOM,
  wheelIntent,
  wheelZoomFactor,
  type WheelInput,
} from './wheel'

const MAC_NOTCH = 4.000244140625

// Recorded in Sift's macOS webview on 2026-10-04 (deltaY of each wheel event)
/** Pinch out: the fingers' own gesture scale ended at 1.700 */
const PINCH_OUT = [
  -0.099, -0.4, -0.891, -0.703, -1.324, -1.199, -1.193, -2.097, -1.66, -1.903, -2.121, -2.063,
  -2.316, -2.255, -2.675, -2.362, -1.834, -2.124, -2.544, -1.611, -1.723, -1.505, -1.331, -1.343,
  -1.198, -1.741, -0.8, -1.778, -1.399, -1.526, -1.399, -1.14, -1.199, -0.999, -1.083,
]
/** Pinch in: gesture scale ended at 0.761 */
const PINCH_IN = [
  0.099, 0.349, 0.473, 0.6, 1.135, 1.399, 1.199, 0.999, 1.199, 1.199, 1.199, 1.199, 1.196, 1.726,
  1.199, 1.067, 1.785, 1.871, 1.767, 1.741, 1.834, 0.8, 1.128, 1.128,
]
/** A quick two-finger flick with its inertia tail, ~1 s, 67 events */
const FLICK: [number, number][] = [
  [0, 1],
  [0, 2],
  [0, 4],
  [0, 3],
  [-1, 3],
  [-1, 3],
  [0, 2],
  [0, 4],
  [0, 4],
  [0, 5],
  [0, 4],
  [0, 4],
  [0, 4],
  [0, 5],
  [0, 5],
  [0, 4],
  [0, 4],
  [0, 4],
  [0, 4],
  [0, 4],
  [0, 4],
  [0, 4],
  [0, 3],
  [0, 3],
  [0, 3],
  [0, 3],
  [0, 3],
  [0, 3],
  [0, 3],
  [0, 3],
  [0, 3],
  [0, 3],
  [0, 3],
  [0, 2],
  [0, 2],
  [0, 2],
  [0, 2],
  [0, 2],
  [0, 2],
  [0, 2],
  [0, 2],
  [0, 2],
  [0, 2],
  [0, 2],
  [0, 2],
  [0, 3],
  [0, 2],
  [0, 2],
  [0, 2],
  [0, 2],
  [0, 1],
  [0, 1],
  [0, 1],
  [0, 1],
  [0, 1],
  [0, 1],
  [0, 1],
  [0, 1],
  [0, 1],
  [0, 1],
  [0, -1],
  [0, -1],
  [0, -2],
  [0, -3],
  [1, -2],
  [2, -2],
  [1, -1],
]

function pinch(deltaY: number): WheelInput {
  return { deltaX: 0, deltaY, deltaMode: 0, ctrlKey: true, wheelDeltaY: Math.round(-3 * deltaY) }
}

function scroll(deltaX: number, deltaY: number): WheelInput {
  return { deltaX, deltaY, deltaMode: 0, ctrlKey: false, wheelDeltaY: -3 * deltaY }
}

function macWheel(notches: number, ctrlKey = false): WheelInput {
  return { deltaX: 0, deltaY: notches * MAC_NOTCH, deltaMode: 0, ctrlKey, wheelDeltaY: 0 }
}

function totalZoom(events: WheelInput[], isMac = true): number {
  return events.reduce((zoom, e) => zoom * wheelZoomFactor(e, isMac), 1)
}

describe('wheelIntent', () => {
  it('reads a ctrlKey wheel event as a trackpad pinch', () => {
    expect(wheelIntent(pinch(-1.3), true)).toBe('pinch')
  })

  it('reads integer pixel deltas without ctrlKey as a trackpad scroll', () => {
    expect(FLICK.every(([x, y]) => wheelIntent(scroll(x, y), true) === 'scroll')).toBe(true)
  })

  it('reads multiples of 4.000244 px on macOS as a mouse wheel', () => {
    expect(wheelIntent(macWheel(1), true)).toBe('mouseWheel')
    expect(wheelIntent(macWheel(-3), true)).toBe('mouseWheel')
  })

  it('reads line-mode deltas as a mouse wheel on any platform', () => {
    const input = { deltaX: 0, deltaY: 3, deltaMode: 1, ctrlKey: false }
    expect(wheelIntent(input, true)).toBe('mouseWheel')
    expect(wheelIntent(input, false)).toBe('mouseWheel')
  })

  it('reads wheelDeltaY multiples of 120 as a mouse wheel outside macOS', () => {
    const notch = { deltaX: 0, deltaY: 100, deltaMode: 0, ctrlKey: false, wheelDeltaY: -120 }
    expect(wheelIntent(notch, false)).toBe('mouseWheel')
  })

  it('reads Ctrl + mouse wheel as a wheel notch, not a pinch', () => {
    const notch = { deltaX: 0, deltaY: 100, deltaMode: 0, ctrlKey: true, wheelDeltaY: -120 }
    expect(wheelIntent(notch, false)).toBe('mouseWheel')
    expect(wheelIntent(macWheel(1, true), true)).toBe('mouseWheel')
  })

  it('reads a horizontal mouse scroll as a scroll so it pans', () => {
    const shifted = { deltaX: MAC_NOTCH, deltaY: 0, deltaMode: 0, ctrlKey: false }
    expect(wheelIntent(shifted, true)).toBe('scroll')
  })
})

describe('wheelZoomFactor', () => {
  it('zooms PINCH_GAIN times the fingers through a recorded pinch out (×1.70 → ×2.2)', () => {
    expect(PINCH_GAIN).toBe(1.5)
    expect(totalZoom(PINCH_OUT.map(pinch))).toBeCloseTo(1.7 ** PINCH_GAIN, 1)
  })

  it('zooms PINCH_GAIN times the fingers through a recorded pinch in (×0.76 → ×0.66)', () => {
    expect(totalZoom(PINCH_IN.map(pinch))).toBeCloseTo(0.76 ** PINCH_GAIN, 1)
  })

  it('makes pinching out and back in by the same amount land where it started', () => {
    const there = PINCH_OUT.map(pinch)
    const back = PINCH_OUT.map((d) => pinch(-d))
    expect(totalZoom([...there, ...back])).toBeCloseTo(1, 10)
  })

  it('caps a single pinch event however large its delta', () => {
    expect(wheelZoomFactor(pinch(-1000), true)).toBeLessThan(2.2)
  })

  it('does not zoom on a trackpad scroll, however long', () => {
    expect(totalZoom(FLICK.map(([x, y]) => scroll(x, y)))).toBe(1)
  })

  it('zooms by WHEEL_NOTCH_ZOOM per mouse wheel notch, about 20%', () => {
    expect(WHEEL_NOTCH_ZOOM).toBe(1.2)
    expect(wheelZoomFactor(macWheel(-1), true)).toBeCloseTo(1.2, 10)
    expect(wheelZoomFactor(macWheel(1), true)).toBeCloseTo(1 / 1.2, 10)
  })

  it('counts an accelerated spin as several notches, at most three per event', () => {
    expect(wheelZoomFactor(macWheel(-2), true)).toBeCloseTo(1.2 ** 2, 10)
    expect(wheelZoomFactor(macWheel(-10), true)).toBeCloseTo(1.2 ** 3, 10)
  })

  it('zooms one notch for a line-mode notch of three lines', () => {
    const input = { deltaX: 0, deltaY: -3, deltaMode: 1, ctrlKey: false }
    expect(wheelZoomFactor(input, false)).toBeCloseTo(1.2, 10)
  })
})
