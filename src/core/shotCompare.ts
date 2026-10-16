// ============================================================
// Sift - Comparing shooting settings
// Lines up the exposure settings of the photos on screen (compare: base
// and current; survey: the burst in view) and finds the ones that differ
// from a reference photo. No Vue, Pinia or Tauri imports.
// ============================================================

import type { ExifData, PhotoPair } from '@/types'
import { captureKeys } from './sorting'

export const ShotParam = {
  Aperture: 'aperture',
  Shutter: 'shutter',
  Iso: 'iso',
  FocalLength: 'focalLength',
  ExposureBias: 'exposureBias',
  Lens: 'lens',
} as const

export type ShotParam = (typeof ShotParam)[keyof typeof ShotParam]

export const SHOT_PARAM_LABELS: Record<ShotParam, string> = {
  aperture: '光圈',
  shutter: '快门',
  iso: 'ISO',
  focalLength: '焦距',
  exposureBias: '曝光补偿',
  lens: '镜头',
}

/** Exposure settings in display order; the lens goes last, it is the longest */
export const SHOT_PARAMS: readonly ShotParam[] = [
  ShotParam.Aperture,
  ShotParam.Shutter,
  ShotParam.Iso,
  ShotParam.FocalLength,
  ShotParam.ExposureBias,
  ShotParam.Lens,
]

export interface ShotCell {
  /** Empty when the photo doesn't record it (or its EXIF is not loaded) */
  text: string
  /** Differs from the reference photo; the reference itself differs when any other does */
  differs: boolean
}

export interface ShotRow {
  param: ShotParam
  label: string
  /** One cell per photo, in the order given */
  cells: ShotCell[]
}

/** How much room a pane's settings strip gives its histogram */
export type StripHistogram = 'full' | 'small' | 'none'

/** Panes at least this wide show the strip's histogram at full size */
export const STRIP_FULL_HISTOGRAM_MIN_PX = 520
/** Panes narrower than this show the settings alone */
export const STRIP_HISTOGRAM_MIN_PX = 360

/** The histogram shrinks before it goes, so narrow windows keep the settings readable */
export function stripHistogram(paneWidth: number): StripHistogram {
  if (paneWidth >= STRIP_FULL_HISTOGRAM_MIN_PX) return 'full'
  if (paneWidth >= STRIP_HISTOGRAM_MIN_PX) return 'small'
  return 'none'
}

/** Typographic minus, as cameras and editors show negative EV */
const MINUS = '−'
const MS_PER_SECOND = 1000
const SECONDS_PER_MINUTE = 60
const SECONDS_PER_HOUR = 3600
/** Below this many seconds an offset is shown to a tenth of a second */
const TENTHS_BELOW_SECONDS = 10

function signOf(value: number): string {
  return value < 0 ? MINUS : '+'
}

/** "+0.7 EV", "−1 EV", "0 EV"; empty when not recorded */
export function formatExposureBias(ev: number | null | undefined): string {
  if (ev == null || !Number.isFinite(ev)) return ''
  const tenths = Math.round(Math.abs(ev) * 10)
  if (tenths === 0) return '0 EV'
  const magnitude = tenths % 10 === 0 ? String(tenths / 10) : (tenths / 10).toFixed(1)
  return `${signOf(ev)}${magnitude} EV`
}

export function formatIso(iso: number): string {
  return iso > 0 ? `ISO ${iso}` : ''
}

/** "1/320s" as read from EXIF → "1/320 s" */
export function formatShutter(shutter: string): string {
  return shutter.replace(/\s*s$/, ' s').trim()
}

/** "46mm" → "46 mm" */
export function formatFocalLength(focal: string): string {
  return focal.replace(/\s*mm$/, ' mm').trim()
}

/** Display text of one setting, or '' when the photo doesn't record it */
export function shotParamText(exif: ExifData | null, param: ShotParam): string {
  if (!exif) return ''
  switch (param) {
    case ShotParam.Aperture:
      return exif.aperture.trim()
    case ShotParam.Shutter:
      return formatShutter(exif.shutterSpeed)
    case ShotParam.Iso:
      return formatIso(exif.iso)
    case ShotParam.FocalLength:
      return formatFocalLength(exif.focalLength)
    case ShotParam.ExposureBias:
      return formatExposureBias(exif.exposureBias)
    case ShotParam.Lens:
      return exif.lens.trim()
  }
}

/** Two recorded values that disagree; a missing value is unknown, not different */
function disagree(a: string, b: string): boolean {
  return a !== '' && b !== '' && a !== b
}

/**
 * The settings of `exifs` lined up row by row. Cells are compared with the
 * photo at `reference` (the base image in compare mode, the focus in survey).
 */
export function compareShots(
  exifs: readonly (ExifData | null)[],
  reference: number,
  params: readonly ShotParam[] = SHOT_PARAMS,
): ShotRow[] {
  return params.map((param) => {
    const texts = exifs.map((exif) => shotParamText(exif, param))
    const refText = texts[reference] ?? ''
    const others = texts.filter((_, i) => i !== reference)
    const cells = texts.map((text, i) => ({
      text,
      differs: i === reference ? others.some((t) => disagree(t, refText)) : disagree(text, refText),
    }))
    return { param, label: SHOT_PARAM_LABELS[param], cells }
  })
}

/**
 * How long after the reference photo each photo was taken, in ms (negative
 * when before); 0 for the reference, null when either time is unknown.
 * Times compare the way sorting does: in UTC only when both record a zone.
 */
export function captureOffsets(pairs: readonly PhotoPair[], reference: number): (number | null)[] {
  const keys = captureKeys(pairs)
  const refKey = pairs[reference] ? keys.get(pairs[reference].id) : undefined
  return pairs.map((pair) => {
    const key = keys.get(pair.id)
    return key === undefined || refKey === undefined ? null : key - refKey
  })
}

/** "+0.4 s", "−12 s", "+2 min 5 s", "+1 h 3 min"; "0 s" when taken together */
export function formatCaptureOffset(ms: number): string {
  const seconds = Math.abs(ms) / MS_PER_SECOND
  const sign = signOf(ms)
  if (seconds < TENTHS_BELOW_SECONDS) {
    const tenths = Math.round(seconds * 10)
    if (tenths === 0) return '0 s'
    return `${sign}${tenths % 10 === 0 ? tenths / 10 : (tenths / 10).toFixed(1)} s`
  }
  const whole = Math.round(seconds)
  if (whole < SECONDS_PER_MINUTE) return `${sign}${whole} s`
  if (whole < SECONDS_PER_HOUR) {
    const rest = whole % SECONDS_PER_MINUTE
    const minutes = `${Math.floor(whole / SECONDS_PER_MINUTE)} min`
    return `${sign}${minutes}${rest ? ` ${rest} s` : ''}`
  }
  const minutes = Math.floor((whole % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE)
  const hours = `${Math.floor(whole / SECONDS_PER_HOUR)} h`
  return `${sign}${hours}${minutes ? ` ${minutes} min` : ''}`
}

/** Camera clock reading as "17:09:32.4" (tenths only when recorded) */
export function formatClockTime(localMs: number): string {
  const date = new Date(localMs)
  const pad = (n: number) => String(n).padStart(2, '0')
  const hms = `${pad(date.getUTCHours())}:${pad(date.getUTCMinutes())}:${pad(date.getUTCSeconds())}`
  const tenths = Math.floor(date.getUTCMilliseconds() / 100)
  return tenths ? `${hms}.${tenths}` : hms
}
