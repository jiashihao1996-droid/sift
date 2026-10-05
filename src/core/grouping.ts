// ============================================================
// Sift - Burst Groups
// Neighbouring photos taken within a few seconds of each other form a
// group (a burst or a re-shot scene). Groups are a view of the current
// order only: they are not saved and archiving ignores them.
// No Vue, Pinia or Tauri imports.
// ============================================================

import { PhotoStatus, type PhotoPair } from '@/types'
import { captureKeys } from './sorting'

/** Gaps offered in the status bar; 0 turns grouping off */
export const BURST_GAP_SECONDS = [0, 1, 2, 5, 10, 30, 60] as const
export const DEFAULT_BURST_GAP_SECONDS = 2

export function isBurstGapSeconds(value: unknown): value is number {
  return (BURST_GAP_SECONDS as readonly unknown[]).includes(value)
}

export interface BurstGroups {
  /** Index of each group's first photo, ascending */
  starts: number[]
  /** Group number of each photo */
  groupOf: number[]
}

/**
 * Split pairs, in display order, into groups: neighbours that both have a
 * capture time no more than `gapMs` apart share a group. Photos without a
 * capture time stand alone. The difference is taken either way round, since
 * file-name order can run against the clock.
 */
export function groupPairs(pairs: readonly PhotoPair[], gapMs: number): BurstGroups {
  const keys = captureKeys(pairs)
  const starts: number[] = []
  const groupOf: number[] = []
  pairs.forEach((pair, i) => {
    const key = keys.get(pair.id)
    const prevKey = i > 0 ? keys.get(pairs[i - 1].id) : undefined
    const isSameBurst =
      gapMs > 0 && key !== undefined && prevKey !== undefined && Math.abs(key - prevKey) <= gapMs
    if (!isSameBurst) starts.push(i)
    groupOf.push(starts.length - 1)
  })
  return { starts, groupOf }
}

/** First index and one past the last index of the group holding `index` */
function groupRange(groups: BurstGroups, index: number): { start: number; end: number } {
  const group = groups.groupOf[index]
  return { start: groups.starts[group], end: groups.starts[group + 1] ?? groups.groupOf.length }
}

/** First photo of the group after the one holding `index`; null in the last group */
export function nextGroupStart(groups: BurstGroups, index: number): number | null {
  return groups.starts[groups.groupOf[index] + 1] ?? null
}

/**
 * Where `[` goes: the start of this group, or from there the start of the
 * previous one; null at the very first photo
 */
export function prevGroupTarget(groups: BurstGroups, index: number): number | null {
  const { start } = groupRange(groups, index)
  if (index > start) return start
  const prevGroup = groups.groupOf[index] - 1
  return prevGroup >= 0 ? groups.starts[prevGroup] : null
}

/** Position within its burst (1-based), or null for a photo on its own */
export function groupPosition(
  groups: BurstGroups,
  index: number,
): { index: number; size: number } | null {
  if (groups.groupOf[index] === undefined) return null
  const { start, end } = groupRange(groups, index)
  return end - start > 1 ? { index: index - start + 1, size: end - start } : null
}

export interface BurstEdge {
  isInBurst: boolean
  isFirst: boolean
  isLast: boolean
  /** A burst starts or ends between this photo and the one before */
  isSpacedBefore: boolean
}

/** How each photo sits in its burst, for drawing group bounds on the strip */
export function burstEdges(groups: BurstGroups): BurstEdge[] {
  const isInBurst = groups.groupOf.map((_, i) => {
    const { start, end } = groupRange(groups, i)
    return end - start > 1
  })
  return groups.groupOf.map((group, i) => {
    const isNewGroup = i > 0 && group !== groups.groupOf[i - 1]
    return {
      isInBurst: isInBurst[i],
      isFirst: isInBurst[i] && (i === 0 || isNewGroup),
      isLast: isInBurst[i] && groups.groupOf[i + 1] !== group,
      isSpacedBefore: isNewGroup && (isInBurst[i] || isInBurst[i - 1]),
    }
  })
}

/** `pick`: Shift+F, keep this photo and drop the rest; `reject`: Shift+X, drop the burst */
export type GroupMarkMode = 'pick' | 'reject'

export interface GroupMarkPlan {
  changes: { index: number; status: PhotoStatus }[]
  starred: number
  deleted: number
  /** Photos of the burst already picked, which stay picked */
  keptStarred: number
}

/**
 * Marks for a one-key decision on the burst holding `index`. Only unmarked
 * photos are marked for deletion; ones already picked or deleted are left as
 * they are, so a second keeper chosen earlier is never lost.
 */
export function planGroupMark(
  pairs: readonly PhotoPair[],
  groups: BurstGroups,
  index: number,
  mode: GroupMarkMode,
): GroupMarkPlan {
  const plan: GroupMarkPlan = { changes: [], starred: 0, deleted: 0, keptStarred: 0 }
  const { start, end } = groupRange(groups, index)
  for (let i = start; i < end; i++) {
    const status = pairs[i].status
    if (mode === 'pick' && i === index) {
      if (status !== PhotoStatus.Starred) {
        plan.changes.push({ index: i, status: PhotoStatus.Starred })
        plan.starred++
      }
    } else if (status === PhotoStatus.Unprocessed) {
      plan.changes.push({ index: i, status: PhotoStatus.Deleted })
      plan.deleted++
    } else if (status === PhotoStatus.Starred) {
      plan.keptStarred++
    }
  }
  return plan
}

/** Toast summing up a one-key decision on a burst */
export function groupMarkToast(
  plan: GroupMarkPlan,
  mode: GroupMarkMode,
): { message: string; type: 'star' | 'delete' | 'info' } {
  if (plan.starred + plan.deleted === 0) {
    return { message: '这一组没有未标记的照片', type: 'info' }
  }
  const parts = [
    plan.starred > 0 ? `★ 精选 ${plan.starred}` : '',
    plan.deleted > 0 ? `删除 ${plan.deleted}` : '',
    plan.keptStarred > 0 ? `保留已精选 ${plan.keptStarred}` : '',
  ]
  return {
    message: parts.filter(Boolean).join(' · '),
    type: mode === 'pick' ? 'star' : 'delete',
  }
}
