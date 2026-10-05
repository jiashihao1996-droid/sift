// ============================================================
// Sift - Wheel input
// What a wheel event asks for: a trackpad pinch or a mouse wheel zooms,
// a two-finger trackpad scroll pans. Measured in Sift's macOS webview:
// - pinch: ctrlKey wheel events every ~8 ms with fractional deltaY of 0.1–3;
//   exp(-ΣdeltaY / 100) reproduces the fingers' own scale (WebKit's gesture
//   events report the same pinch, so they are not needed)
// - two-finger scroll: integer pixel deltas of 1–5 every ~8 ms plus an inertia tail
// ============================================================

export type WheelIntent = 'pinch' | 'mouseWheel' | 'scroll'

/** The parts of a WheelEvent this module reads; wheelDeltaY is non-standard */
export interface WheelInput {
  deltaX: number
  deltaY: number
  deltaMode: number
  ctrlKey: boolean
  wheelDeltaY?: number
}

/** exp(-deltaY × 1/100) is the fingers' own scale */
const FINGER_ZOOM_RATE = 0.01
/** Pinch zooms this much faster than the fingers move: 1:1 felt too slow */
export const PINCH_GAIN = 1.5
/** Caps a single pinch event at about ×2.1 in case a webview reports a jump */
const MAX_PINCH_DELTA = 50

/** Zoom per mouse wheel notch, close to the 1.25 of the +/- keys */
export const WHEEL_NOTCH_ZOOM = 1.2
/** An accelerated spin reports several notches in one event; more would jump */
const MAX_NOTCHES_PER_EVENT = 3

/** macOS WebKit reports a mouse wheel notch as a multiple of this many pixels */
const MAC_NOTCH_PX = 4.000244140625
/** Chromium-based webviews (Windows) report a notch as wheelDeltaY ±120 */
const LEGACY_NOTCH = 120
/** A line-mode notch scrolls three lines */
const LINES_PER_NOTCH = 3

const DOM_DELTA_PIXEL = 0
const DOM_DELTA_LINE = 1

function isMultipleOf(value: number, step: number): boolean {
  const n = Math.abs(value) / step
  return n >= 1 && Math.abs(n - Math.round(n)) < 1e-6
}

/**
 * Number of mouse wheel notches in the event, or 0 when it does not look like one.
 * Trackpads report integer pixels in WebKit, so a multiple of 4.000244 is a notch.
 */
function wheelNotches(input: WheelInput, isMac: boolean): number {
  const { deltaX, deltaY, deltaMode, wheelDeltaY } = input
  if (deltaMode === DOM_DELTA_LINE) return Math.abs(deltaY) / LINES_PER_NOTCH
  if (deltaMode !== DOM_DELTA_PIXEL) return Math.abs(deltaY) > 0 ? 1 : 0
  if (deltaX !== 0) return 0
  if (isMac) return isMultipleOf(deltaY, MAC_NOTCH_PX) ? Math.abs(deltaY) / MAC_NOTCH_PX : 0
  return wheelDeltaY && isMultipleOf(wheelDeltaY, LEGACY_NOTCH)
    ? Math.abs(wheelDeltaY) / LEGACY_NOTCH
    : 0
}

/**
 * Tells a mouse wheel from a trackpad. Anything unrecognised counts as a trackpad
 * scroll, so a misread trackpad pans instead of zooming wildly. A mouse wheel is
 * checked before ctrlKey: Ctrl+wheel on Windows is a notch, not a pinch.
 */
export function wheelIntent(input: WheelInput, isMac: boolean): WheelIntent {
  if (wheelNotches(input, isMac) > 0) return 'mouseWheel'
  if (input.ctrlKey) return 'pinch'
  return 'scroll'
}

/** Zoom multiplier for a pinch or mouse wheel event; 1 for a scroll, which pans */
export function wheelZoomFactor(input: WheelInput, isMac: boolean): number {
  const intent = wheelIntent(input, isMac)
  if (intent === 'scroll' || input.deltaY === 0) return 1
  if (intent === 'pinch') {
    const delta = Math.max(-MAX_PINCH_DELTA, Math.min(MAX_PINCH_DELTA, input.deltaY))
    return Math.exp(-delta * FINGER_ZOOM_RATE * PINCH_GAIN)
  }
  const notches = Math.min(MAX_NOTCHES_PER_EVENT, wheelNotches(input, isMac))
  return WHEEL_NOTCH_ZOOM ** (input.deltaY > 0 ? -notches : notches)
}
