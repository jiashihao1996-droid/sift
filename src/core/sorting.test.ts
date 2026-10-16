import { describe, expect, it } from 'vitest'
import { PhotoStatus, type CaptureTime, type PhotoPair } from '@/types'
import {
  SortOrder,
  countWithoutCaptureTime,
  fileNameRanks,
  isSortOrder,
  sortPairs,
} from './sorting'

const SECOND = 1000
const HOUR = 3600 * SECOND

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

const ids = (pairs: readonly PhotoPair[]) => pairs.map((p) => p.id)

/** Sort `pairs`, given in file-name order (as the scan returns them) */
function sortScanned(pairs: PhotoPair[], order: SortOrder) {
  return ids(sortPairs(pairs, order, fileNameRanks(pairs)))
}

describe('sortPairs by capture time', () => {
  it('orders a counter rollover by when the photos were taken', () => {
    const pairs = [pair('DSC00001', at(10 * SECOND)), pair('DSC09999', at(5 * SECOND))]
    expect(sortScanned(pairs, SortOrder.CaptureTime)).toEqual(['DSC09999', 'DSC00001'])
  })

  it('interleaves two cameras shooting the same scene', () => {
    const pairs = [
      pair('A_001', at(1 * SECOND)),
      pair('A_002', at(3 * SECOND)),
      pair('B_001', at(2 * SECOND)),
      pair('B_002', at(4 * SECOND)),
    ]
    expect(sortScanned(pairs, SortOrder.CaptureTime)).toEqual(['A_001', 'B_001', 'A_002', 'B_002'])
  })

  it('keeps file-name order between photos taken at the same moment', () => {
    const pairs = [pair('IMG_1', at(SECOND)), pair('IMG_2', at(SECOND)), pair('IMG_3', at(0))]
    expect(sortScanned(pairs, SortOrder.CaptureTime)).toEqual(['IMG_3', 'IMG_1', 'IMG_2'])
  })

  it('puts photos without a capture time last, in file-name order', () => {
    const pairs = [
      pair('IMG_1', null),
      pair('IMG_2', at(2 * SECOND)),
      pair('IMG_3', null),
      pair('IMG_4', at(1 * SECOND)),
    ]
    expect(sortScanned(pairs, SortOrder.CaptureTime)).toEqual(['IMG_4', 'IMG_2', 'IMG_1', 'IMG_3'])
  })

  it('compares in UTC when every photo recorded its time zone', () => {
    // Camera A set to UTC+8, camera B to UTC+9: B's 11:00 came before A's 10:30
    const pairs = [pair('A', at(10.5 * HOUR, 480)), pair('B', at(11 * HOUR, 540))]
    expect(sortScanned(pairs, SortOrder.CaptureTime)).toEqual(['B', 'A'])
  })

  it('compares camera clocks when any photo lacks a time zone', () => {
    const pairs = [
      pair('A', at(10.5 * HOUR, 480)),
      pair('B', at(11 * HOUR, 540)),
      pair('C', at(12 * HOUR, null)),
    ]
    expect(sortScanned(pairs, SortOrder.CaptureTime)).toEqual(['A', 'B', 'C'])
  })

  it('returns a new array and leaves the input alone', () => {
    const pairs = [pair('IMG_1', at(2)), pair('IMG_2', at(1))]
    const sorted = sortPairs(pairs, SortOrder.CaptureTime, fileNameRanks(pairs))
    expect(sorted).not.toBe(pairs)
    expect(ids(pairs)).toEqual(['IMG_1', 'IMG_2'])
  })
})

describe('sortPairs by file name', () => {
  it('restores the scan order after sorting by capture time', () => {
    const scanned = [pair('IMG_1', at(3)), pair('IMG_2', null), pair('IMG_3', at(1))]
    const ranks = fileNameRanks(scanned)
    const byTime = sortPairs(scanned, SortOrder.CaptureTime, ranks)

    expect(ids(sortPairs(byTime, SortOrder.FileName, ranks))).toEqual(['IMG_1', 'IMG_2', 'IMG_3'])
  })

  it('keeps working after pairs leave the session', () => {
    const scanned = [pair('IMG_1', null), pair('IMG_2', null), pair('IMG_3', null)]
    const ranks = fileNameRanks(scanned)
    const remaining = [scanned[2], scanned[0]]

    expect(ids(sortPairs(remaining, SortOrder.FileName, ranks))).toEqual(['IMG_1', 'IMG_3'])
  })
})

describe('countWithoutCaptureTime', () => {
  it('counts the photos that do not record when they were taken', () => {
    const pairs = [pair('IMG_1', null), pair('IMG_2', at(0)), pair('IMG_3', null)]
    expect(countWithoutCaptureTime(pairs)).toBe(2)
  })
})

describe('isSortOrder', () => {
  it('accepts only known orders', () => {
    expect(isSortOrder('captureTime')).toBe(true)
    expect(isSortOrder('fileName')).toBe(true)
    expect(isSortOrder('random')).toBe(false)
    expect(isSortOrder(null)).toBe(false)
  })
})
