import { describe, expect, it } from 'vitest'
import { PhotoStatus, type CaptureTime, type ExifData, type PhotoPair } from '@/types'
import {
  ShotParam,
  captureOffsets,
  compareShots,
  formatCaptureOffset,
  formatClockTime,
  formatExposureBias,
  shotParamText,
  stripHistogram,
} from './shotCompare'

const SECOND = 1000
const HOUR = 3600 * SECOND

function exif(overrides: Partial<ExifData> = {}): ExifData {
  return {
    camera: 'ILCE-7RM5',
    lens: 'FE 24-70mm F2.8 GM II',
    iso: 100,
    aperture: 'f/9',
    shutterSpeed: '1/320s',
    focalLength: '46mm',
    exposureBias: 0,
    dateTaken: '2026-10-02 17:09:32',
    dimensions: { width: 9504, height: 6336 },
    fileSize: 1024,
    ...overrides,
  }
}

function pair(name: string, captureTime: CaptureTime | null): PhotoPair {
  return {
    id: name,
    jpgPath: `/photos/${name}.JPG`,
    rawPath: null,
    rawFormat: null,
    status: PhotoStatus.Unprocessed,
    thumbnailPath: null,
    dominantColor: null,
    source: 'jpg',
    xmpPaths: [],
    captureTime,
    isArchivedPick: false,
  }
}

const at = (localMs: number, offsetMinutes: number | null = null): CaptureTime => ({
  localMs,
  offsetMinutes,
})

describe('formatExposureBias', () => {
  it('signs positive and negative compensation with one decimal', () => {
    expect(formatExposureBias(2 / 3)).toBe('+0.7 EV')
    expect(formatExposureBias(-1 / 3)).toBe('−0.3 EV')
  })

  it('drops the decimal for whole stops', () => {
    expect(formatExposureBias(1)).toBe('+1 EV')
    expect(formatExposureBias(-2)).toBe('−2 EV')
  })

  it('shows no compensation as 0 EV without a sign', () => {
    expect(formatExposureBias(0)).toBe('0 EV')
    expect(formatExposureBias(-0.01)).toBe('0 EV')
  })

  it('is empty when the camera did not record it', () => {
    expect(formatExposureBias(null)).toBe('')
    expect(formatExposureBias(Number.NaN)).toBe('')
  })
})

describe('shotParamText', () => {
  it('spaces units and prefixes ISO', () => {
    const e = exif()
    expect(shotParamText(e, ShotParam.Shutter)).toBe('1/320 s')
    expect(shotParamText(e, ShotParam.FocalLength)).toBe('46 mm')
    expect(shotParamText(e, ShotParam.Iso)).toBe('ISO 100')
    expect(shotParamText(e, ShotParam.Aperture)).toBe('f/9')
  })

  it('is empty for settings the photo does not record', () => {
    const e = exif({ iso: 0, shutterSpeed: '', focalLength: '', lens: '', exposureBias: null })
    for (const param of Object.values(ShotParam)) {
      if (param === ShotParam.Aperture) continue
      expect(shotParamText(e, param)).toBe('')
    }
    expect(shotParamText(null, ShotParam.Aperture)).toBe('')
  })
})

describe('compareShots', () => {
  it('flags a differing setting on both photos of a compare', () => {
    const rows = compareShots([exif(), exif({ shutterSpeed: '1/500s' })], 0)
    const shutter = rows.find((r) => r.param === ShotParam.Shutter)
    expect(shutter?.cells).toEqual([
      { text: '1/320 s', differs: true },
      { text: '1/500 s', differs: true },
    ])
    const iso = rows.find((r) => r.param === ShotParam.Iso)
    expect(iso?.cells.map((c) => c.differs)).toEqual([false, false])
  })

  it('compares every photo with the reference, not with each other', () => {
    const rows = compareShots(
      [exif({ iso: 100 }), exif({ iso: 200 }), exif({ iso: 100 }), exif({ iso: 200 })],
      2,
    )
    const iso = rows.find((r) => r.param === ShotParam.Iso)
    expect(iso?.cells.map((c) => c.differs)).toEqual([false, true, true, true])
  })

  it('does not flag the reference when every other photo agrees with it', () => {
    const rows = compareShots([exif(), exif(), exif()], 1)
    expect(rows.every((r) => r.cells.every((c) => !c.differs))).toBe(true)
  })

  it('treats a missing value as unknown rather than different', () => {
    const rows = compareShots([exif(), exif({ exposureBias: null }), null], 0)
    const bias = rows.find((r) => r.param === ShotParam.ExposureBias)
    expect(bias?.cells).toEqual([
      { text: '0 EV', differs: false },
      { text: '', differs: false },
      { text: '', differs: false },
    ])
  })

  it('lists the settings in display order with their labels', () => {
    const rows = compareShots([exif()], 0)
    expect(rows.map((r) => r.label)).toEqual(['光圈', '快门', 'ISO', '焦距', '曝光补偿', '镜头'])
  })

  it('can be limited to some settings', () => {
    const rows = compareShots([exif()], 0, [ShotParam.Iso])
    expect(rows.map((r) => r.param)).toEqual([ShotParam.Iso])
  })
})

describe('captureOffsets', () => {
  it('measures each photo from the reference', () => {
    const pairs = [
      pair('a', at(10 * SECOND)),
      pair('b', at(10.4 * SECOND)),
      pair('c', at(8 * SECOND)),
    ]
    expect(captureOffsets(pairs, 0)).toEqual([0, 0.4 * SECOND, -2 * SECOND])
  })

  it('compares in UTC when both photos record their time zone', () => {
    const pairs = [pair('a', at(10 * HOUR, 60)), pair('b', at(9 * HOUR + 5 * SECOND, 0))]
    expect(captureOffsets(pairs, 0)).toEqual([0, 5 * SECOND])
  })

  it('compares camera clocks as they read when a time zone is missing', () => {
    const pairs = [pair('a', at(10 * HOUR, 60)), pair('b', at(10 * HOUR + 3 * SECOND, null))]
    expect(captureOffsets(pairs, 0)).toEqual([0, 3 * SECOND])
  })

  it('is null for photos without a capture time, and for all when the reference has none', () => {
    expect(captureOffsets([pair('a', at(0)), pair('b', null)], 0)).toEqual([0, null])
    expect(captureOffsets([pair('a', null), pair('b', at(0))], 0)).toEqual([null, null])
  })
})

describe('formatCaptureOffset', () => {
  it('shows tenths under ten seconds', () => {
    expect(formatCaptureOffset(400)).toBe('+0.4 s')
    expect(formatCaptureOffset(-1250)).toBe('−1.3 s')
    expect(formatCaptureOffset(2000)).toBe('+2 s')
  })

  it('shows taken together as 0 s', () => {
    expect(formatCaptureOffset(0)).toBe('0 s')
    expect(formatCaptureOffset(-40)).toBe('0 s')
  })

  it('switches to minutes and hours for longer gaps', () => {
    expect(formatCaptureOffset(42 * SECOND)).toBe('+42 s')
    expect(formatCaptureOffset(125 * SECOND)).toBe('+2 min 5 s')
    expect(formatCaptureOffset(-120 * SECOND)).toBe('−2 min')
    expect(formatCaptureOffset(HOUR + 180 * SECOND)).toBe('+1 h 3 min')
    expect(formatCaptureOffset(2 * HOUR)).toBe('+2 h')
  })
})

describe('formatClockTime', () => {
  it('reads the camera clock without a time zone', () => {
    const ms = Date.UTC(2026, 9, 2, 17, 9, 32)
    expect(formatClockTime(ms)).toBe('17:09:32')
    expect(formatClockTime(ms + 450)).toBe('17:09:32.4')
  })
})

describe('stripHistogram', () => {
  it('shrinks the histogram in narrower panes before dropping it', () => {
    expect(stripHistogram(700)).toBe('full')
    expect(stripHistogram(520)).toBe('full')
    expect(stripHistogram(450)).toBe('small')
    expect(stripHistogram(360)).toBe('small')
    expect(stripHistogram(300)).toBe('none')
  })
})
