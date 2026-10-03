// ============================================================
// Sift - Review Rules
// Pure culling logic (status toggling, navigation targets, counts).
// No Vue, Pinia or Tauri imports — keep it that way so it stays
// trivially testable.
// ============================================================

import { PhotoStatus, type PhotoPair, type SlideDirection, type UndoAction } from '@/types'

export type MarkType = UndoAction['type']

const MARK_STATUS: Record<MarkType, PhotoStatus> = {
  star: PhotoStatus.Starred,
  delete: PhotoStatus.Deleted,
}

/**
 * Status after pressing a mark key. Pressing the key of the status a photo
 * already has unmarks it; the other key switches to it.
 */
export function toggledStatus(current: PhotoStatus, type: MarkType): PhotoStatus {
  const target = MARK_STATUS[type]
  return current === target ? PhotoStatus.Unprocessed : target
}

/**
 * Where to go after pressing a mark key: marking moves on to the next photo
 * (staying on the last one), unmarking stays so the photo can be decided again.
 */
export function indexAfterMark(status: PhotoStatus, currentIndex: number, total: number): number {
  if (status === PhotoStatus.Unprocessed) return currentIndex
  return Math.min(currentIndex + 1, total - 1)
}

/** Slide direction for moving between photos: forward slides left, backward slides right. */
export function slideDirectionTo(fromIndex: number, toIndex: number): SlideDirection {
  return toIndex < fromIndex ? 'right' : 'left'
}

export type StatusCounts = Record<PhotoStatus, number>

export function countByStatus(pairs: readonly PhotoPair[]): StatusCounts {
  const counts: StatusCounts = {
    [PhotoStatus.Unprocessed]: 0,
    [PhotoStatus.Starred]: 0,
    [PhotoStatus.Deleted]: 0,
  }
  for (const pair of pairs) counts[pair.status]++
  return counts
}

/** How far through the folder the current photo is, as a whole percentage. */
export function positionPercent(currentIndex: number, total: number): number {
  if (total === 0) return 0
  return Math.round(((currentIndex + 1) / total) * 100)
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
