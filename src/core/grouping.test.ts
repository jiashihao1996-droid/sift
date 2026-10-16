import { describe, expect, it } from 'vitest'
import { PhotoStatus, type PhotoPair } from '@/types'
import {
  burstEdges,
  groupMarkToast,
  groupPairs,
  groupPosition,
  isBurstGapSeconds,
  nextGroupStart,
  planGroupMark,
  prevGroupTarget,
} from './grouping'

const { Unprocessed: U, Starred: S, Deleted: D } = PhotoStatus
const SECOND = 1000

/** Photos taken at the given seconds (null: no capture time), all unmarked */
function shot(seconds: (number | null)[], statuses: PhotoStatus[] = []): PhotoPair[] {
  return seconds.map((s, i) => ({
    id: `p${i}`,
    jpgPath: `/photos/IMG_${i}.JPG`,
    rawPath: null,
    rawFormat: null,
    status: statuses[i] ?? U,
    thumbnailPath: null,
    dominantColor: null,
    source: 'jpg',
    xmpPaths: [],
    captureTime: s === null ? null : { localMs: s * SECOND, offsetMinutes: null },
    isArchivedPick: false,
  }))
}

const GAP = 2 * SECOND

describe('groupPairs', () => {
  it('splits where neighbours are further apart than the gap', () => {
    const groups = groupPairs(shot([0, 0.5, 2.5, 10, 11, 30]), GAP)
    expect(groups.starts).toEqual([0, 3, 5])
    expect(groups.groupOf).toEqual([0, 0, 0, 1, 1, 2])
  })

  it('counts a gap exactly as long as the setting as the same burst', () => {
    expect(groupPairs(shot([0, 2]), GAP).starts).toEqual([0])
  })

  it('compares neighbours whichever way the clock went, as file-name order may run backwards', () => {
    expect(groupPairs(shot([5, 4, 3]), GAP).starts).toEqual([0])
  })

  it('keeps photos without a capture time apart', () => {
    expect(groupPairs(shot([0, null, null, 1]), GAP).starts).toEqual([0, 1, 2, 3])
  })

  it('groups nothing when the gap is off', () => {
    expect(groupPairs(shot([0, 0, 0]), 0).starts).toEqual([0, 1, 2])
  })

  it('compares in UTC when every photo recorded its time zone', () => {
    const pairs = shot([0, 0])
    // Same camera clock reading, an hour apart in real time
    pairs[0].captureTime = { localMs: 0, offsetMinutes: 0 }
    pairs[1].captureTime = { localMs: 0, offsetMinutes: 60 }
    expect(groupPairs(pairs, GAP).starts).toEqual([0, 1])
  })

  it('has no groups for an empty session', () => {
    expect(groupPairs([], GAP)).toEqual({ starts: [], groupOf: [] })
  })
})

describe('navigation between groups', () => {
  const groups = groupPairs(shot([0, 1, 2, 10, 20, 21]), GAP) // [0-2] [3] [4-5]

  it('jumps to the first photo of the next group', () => {
    expect(nextGroupStart(groups, 0)).toBe(3)
    expect(nextGroupStart(groups, 2)).toBe(3)
    expect(nextGroupStart(groups, 3)).toBe(4)
  })

  it('has no next group in the last one', () => {
    expect(nextGroupStart(groups, 4)).toBeNull()
    expect(nextGroupStart(groups, 5)).toBeNull()
  })

  it('goes back to the start of the group, then to the previous group', () => {
    expect(prevGroupTarget(groups, 5)).toBe(4)
    expect(prevGroupTarget(groups, 4)).toBe(3)
    expect(prevGroupTarget(groups, 3)).toBe(0)
    expect(prevGroupTarget(groups, 0)).toBeNull()
  })
})

describe('groupPosition', () => {
  const groups = groupPairs(shot([0, 1, 2, 10]), GAP)

  it('tells where in its burst a photo is', () => {
    expect(groupPosition(groups, 1)).toEqual({ index: 2, size: 3 })
  })

  it('is null for a photo on its own', () => {
    expect(groupPosition(groups, 3)).toBeNull()
  })
})

describe('burstEdges', () => {
  it('marks members and ends of bursts only', () => {
    const edges = burstEdges(groupPairs(shot([0, 1, 10, 20, 21, 22]), GAP))
    expect(edges.map((e) => e.isInBurst)).toEqual([true, true, false, true, true, true])
    expect(edges.map((e) => e.isFirst)).toEqual([true, false, false, true, false, false])
    expect(edges.map((e) => e.isLast)).toEqual([false, true, false, false, false, true])
  })

  it('spaces bursts apart from their neighbours but not lone photos from each other', () => {
    const edges = burstEdges(groupPairs(shot([0, 1, 10, 20, 30, 31]), GAP))
    expect(edges.map((e) => e.isSpacedBefore)).toEqual([false, false, true, false, true, false])
  })
})

describe('planGroupMark', () => {
  const statusesAfter = (pairs: PhotoPair[], plan: ReturnType<typeof planGroupMark>) => {
    const next = pairs.map((p) => p.status)
    for (const c of plan.changes) next[c.index] = c.status
    return next
  }

  it('picks this photo and deletes the rest of its burst', () => {
    const pairs = shot([0, 1, 2, 10])
    const plan = planGroupMark(pairs, groupPairs(pairs, GAP), 1, 'pick')

    expect(statusesAfter(pairs, plan)).toEqual([D, S, D, U])
    expect(plan).toMatchObject({ starred: 1, deleted: 2, keptStarred: 0 })
  })

  it('leaves photos already marked in the burst alone', () => {
    const pairs = shot([0, 1, 2, 3], [S, U, D, U])
    const plan = planGroupMark(pairs, groupPairs(pairs, GAP), 1, 'pick')

    expect(statusesAfter(pairs, plan)).toEqual([S, S, D, D])
    expect(plan).toMatchObject({ starred: 1, deleted: 1, keptStarred: 1 })
  })

  it('picks a photo marked for deletion', () => {
    const pairs = shot([0, 1], [U, D])
    const plan = planGroupMark(pairs, groupPairs(pairs, GAP), 1, 'pick')
    expect(statusesAfter(pairs, plan)).toEqual([D, S])
  })

  it('changes nothing about a photo already picked', () => {
    const pairs = shot([0, 1], [U, S])
    const plan = planGroupMark(pairs, groupPairs(pairs, GAP), 1, 'pick')
    expect(plan.changes).toEqual([{ index: 0, status: D }])
    expect(plan.starred).toBe(0)
  })

  it('rejects every unmarked photo of the burst', () => {
    const pairs = shot([0, 1, 2, 10], [U, S, U, U])
    const plan = planGroupMark(pairs, groupPairs(pairs, GAP), 0, 'reject')

    expect(statusesAfter(pairs, plan)).toEqual([D, S, D, U])
    expect(plan).toMatchObject({ starred: 0, deleted: 2, keptStarred: 1 })
  })

  it('acts on the photo alone when it is not in a burst', () => {
    const pairs = shot([0, 10])
    expect(planGroupMark(pairs, groupPairs(pairs, GAP), 1, 'pick').changes).toEqual([
      { index: 1, status: S },
    ])
  })
})

describe('groupMarkToast', () => {
  it('sums up what changed', () => {
    expect(groupMarkToast({ changes: [], starred: 1, deleted: 5, keptStarred: 1 }, 'pick')).toEqual(
      { message: '★ 精选 1 · 删除 5 · 保留已精选 1', type: 'star' },
    )
    expect(
      groupMarkToast({ changes: [], starred: 0, deleted: 3, keptStarred: 0 }, 'reject'),
    ).toEqual({ message: '删除 3', type: 'delete' })
  })

  it('says so when there was nothing to change', () => {
    expect(
      groupMarkToast({ changes: [], starred: 0, deleted: 0, keptStarred: 2 }, 'reject'),
    ).toEqual({ message: '这一组没有未标记的照片', type: 'info' })
  })
})

describe('isBurstGapSeconds', () => {
  it('accepts only the offered gaps', () => {
    expect(isBurstGapSeconds(2)).toBe(true)
    expect(isBurstGapSeconds(0)).toBe(true)
    expect(isBurstGapSeconds(3)).toBe(false)
    expect(isBurstGapSeconds('2')).toBe(false)
  })
})
