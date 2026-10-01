// ============================================================
// Sift - Review Rules
// Pure culling logic (status toggling, navigation targets, counts).
// No Vue, Pinia or Tauri imports — keep it that way so it stays
// trivially testable.
// ============================================================

import { PhotoStatus, type PhotoPair, type UndoAction } from '@/types'

export type MarkType = UndoAction['type']

const MARK_STATUS: Record<MarkType, PhotoStatus> = {
  star: PhotoStatus.Starred,
  delete: PhotoStatus.Deleted,
  skip: PhotoStatus.Skipped,
}

/**
 * Status after pressing a mark key. Pressing the key of the status a photo
 * already has toggles it back to unprocessed; any other key switches to it.
 */
export function toggledStatus(current: PhotoStatus, type: MarkType): PhotoStatus {
  const target = MARK_STATUS[type]
  return current === target ? PhotoStatus.Unprocessed : target
}

/**
 * First unprocessed photo after `fromIndex`, wrapping around to the start.
 * Returns null when every other photo is already processed.
 */
export function findNextUnprocessed(pairs: readonly PhotoPair[], fromIndex: number): number | null {
  const isUnprocessed = (i: number) => pairs[i].status === PhotoStatus.Unprocessed
  for (let i = fromIndex + 1; i < pairs.length; i++) {
    if (isUnprocessed(i)) return i
  }
  for (let i = 0; i < fromIndex; i++) {
    if (isUnprocessed(i)) return i
  }
  return null
}

/** Where to go after marking: next unprocessed photo, else the next photo, else stay. */
export function indexAfterMark(pairs: readonly PhotoPair[], currentIndex: number): number {
  return findNextUnprocessed(pairs, currentIndex) ?? Math.min(currentIndex + 1, pairs.length - 1)
}

export type StatusCounts = Record<PhotoStatus, number>

export function countByStatus(pairs: readonly PhotoPair[]): StatusCounts {
  const counts: StatusCounts = {
    [PhotoStatus.Unprocessed]: 0,
    [PhotoStatus.Starred]: 0,
    [PhotoStatus.Deleted]: 0,
    [PhotoStatus.Skipped]: 0,
  }
  for (const pair of pairs) counts[pair.status]++
  return counts
}

/** Share of photos that have any mark, as a whole percentage. */
export function progressPercent(counts: StatusCounts): number {
  const total = Object.values(counts).reduce((sum, n) => sum + n, 0)
  if (total === 0) return 0
  return Math.round(((total - counts[PhotoStatus.Unprocessed]) / total) * 100)
}

/**
 * Split items into a small first batch (fills the visible viewport fast)
 * followed by fixed-size chunks.
 */
export function planBatches<T>(items: readonly T[], firstBatch: number, chunkSize: number): T[][] {
  const batches = [items.slice(0, firstBatch)]
  for (let i = firstBatch; i < items.length; i += chunkSize) {
    batches.push(items.slice(i, i + chunkSize))
  }
  return batches.filter((batch) => batch.length > 0)
}
