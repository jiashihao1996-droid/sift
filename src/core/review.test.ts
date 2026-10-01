import { describe, expect, it } from 'vitest'
import { PhotoStatus, type PhotoPair } from '@/types'
import {
  countByStatus,
  findNextUnprocessed,
  indexAfterMark,
  planBatches,
  progressPercent,
  toggledStatus,
} from './review'

const { Unprocessed: U, Starred: S, Deleted: D, Skipped: K } = PhotoStatus

function pairs(statuses: PhotoStatus[]): PhotoPair[] {
  return statuses.map((status, i) => ({
    id: `p${i}`,
    jpgPath: `/p/${i}.jpg`,
    rawPath: null,
    rawFormat: null,
    status,
    source: 'jpg',
    xmpPaths: [],
  }))
}

describe('toggledStatus', () => {
  it.each([
    [U, 'star', S],
    [U, 'delete', D],
    [U, 'skip', K],
    [S, 'delete', D],
    [D, 'skip', K],
  ] as const)('%s + %s -> %s', (from, type, to) => {
    expect(toggledStatus(from, type)).toBe(to)
  })

  it.each([
    [S, 'star'],
    [D, 'delete'],
    [K, 'skip'],
  ] as const)('pressing %s again on %s toggles back to unprocessed', (from, type) => {
    expect(toggledStatus(from, type)).toBe(U)
  })
})

describe('findNextUnprocessed', () => {
  it('searches forward first', () => {
    expect(findNextUnprocessed(pairs([U, S, U, U]), 0)).toBe(2)
  })

  it('wraps around to the start', () => {
    expect(findNextUnprocessed(pairs([U, S, S]), 1)).toBe(0)
  })

  it('ignores the current photo', () => {
    expect(findNextUnprocessed(pairs([S, U, S]), 1)).toBeNull()
  })

  it('returns null when everything is processed', () => {
    expect(findNextUnprocessed(pairs([S, D, K]), 0)).toBeNull()
  })
})

describe('indexAfterMark', () => {
  it('prefers the next unprocessed photo', () => {
    expect(indexAfterMark(pairs([S, S, S, U]), 0)).toBe(3)
  })

  it('falls back to the next photo', () => {
    expect(indexAfterMark(pairs([S, S, S]), 0)).toBe(1)
  })

  it('stays on the last photo', () => {
    expect(indexAfterMark(pairs([S, S]), 1)).toBe(1)
    expect(indexAfterMark(pairs([S]), 0)).toBe(0)
  })
})

describe('countByStatus / progressPercent', () => {
  it('counts each status in one pass', () => {
    expect(countByStatus(pairs([U, S, S, D, K]))).toEqual({
      [U]: 1,
      [S]: 2,
      [D]: 1,
      [K]: 1,
    })
  })

  it('rounds the processed share to a whole percentage', () => {
    expect(progressPercent(countByStatus(pairs([S, U, U])))).toBe(33)
    expect(progressPercent(countByStatus(pairs([S, D, K])))).toBe(100)
  })

  it('is zero for an empty session', () => {
    expect(progressPercent(countByStatus([]))).toBe(0)
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
