import { describe, expect, it } from 'vitest'
import { PhotoStatus, type PhotoPair } from '@/types'
import {
  countByStatus,
  indexAfterMark,
  planBatches,
  positionPercent,
  slideDirectionTo,
  toggledStatus,
} from './review'

const { Unprocessed: U, Starred: S, Deleted: D } = PhotoStatus

function pairs(statuses: PhotoStatus[]): PhotoPair[] {
  return statuses.map((status, i) => ({
    id: `p${i}`,
    jpgPath: `/p/${i}.jpg`,
    rawPath: null,
    rawFormat: null,
    status,
    thumbnailPath: null,
    dominantColor: null,
    source: 'jpg',
    xmpPaths: [],
  }))
}

describe('toggledStatus', () => {
  it.each([
    [U, 'star', S],
    [U, 'delete', D],
    [S, 'delete', D],
    [D, 'star', S],
  ] as const)('%s + %s -> %s', (from, type, to) => {
    expect(toggledStatus(from, type)).toBe(to)
  })

  it.each([
    [S, 'star'],
    [D, 'delete'],
  ] as const)('pressing the key of %s again (%s) unmarks it', (from, type) => {
    expect(toggledStatus(from, type)).toBe(U)
  })
})

describe('indexAfterMark', () => {
  it('moves on to the next photo after marking', () => {
    expect(indexAfterMark(S, 0, 3)).toBe(1)
    expect(indexAfterMark(D, 1, 3)).toBe(2)
  })

  it('stays on the last photo', () => {
    expect(indexAfterMark(S, 2, 3)).toBe(2)
  })

  it('stays put after unmarking, so the photo can be decided again', () => {
    expect(indexAfterMark(U, 1, 3)).toBe(1)
  })
})

describe('countByStatus', () => {
  it('counts each status in one pass', () => {
    expect(countByStatus(pairs([U, S, S, D]))).toEqual({ [U]: 1, [S]: 2, [D]: 1 })
  })
})

describe('positionPercent', () => {
  it('is the share of the folder up to and including the current photo', () => {
    expect(positionPercent(0, 3)).toBe(33)
    expect(positionPercent(2, 3)).toBe(100)
  })

  it('is zero for an empty session', () => {
    expect(positionPercent(0, 0)).toBe(0)
  })
})

describe('planBatches', () => {
  it('splits into a first batch then fixed chunks', () => {
    const sizes = planBatches(
      Array.from({ length: 120 }, (_, i) => i),
      10,
      50,
    ).map((b) => b.length)
    expect(sizes).toEqual([10, 50, 50, 10])
  })

  it('keeps order and drops empty batches', () => {
    expect(planBatches([1, 2, 3], 10, 50)).toEqual([[1, 2, 3]])
    expect(planBatches([], 10, 50)).toEqual([])
  })
})

describe('slideDirectionTo', () => {
  it('slides left when moving forward', () => {
    expect(slideDirectionTo(2, 5)).toBe('left')
  })

  it('slides right when moving backward, e.g. wrapping to an earlier photo', () => {
    expect(slideDirectionTo(5, 0)).toBe('right')
  })
})
