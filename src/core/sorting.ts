// ============================================================
// Sift - Sort Order
// Photos are shown by capture time or by file name. The scan returns
// them by file name; capture time fixes the order when two cameras,
// two cards or a counter rollover (DSC09999 → DSC00001) mix it up.
// No Vue, Pinia or Tauri imports.
// ============================================================

import type { PhotoPair } from '@/types'

export const SortOrder = {
  CaptureTime: 'captureTime',
  FileName: 'fileName',
} as const

export type SortOrder = (typeof SortOrder)[keyof typeof SortOrder]

const MS_PER_MINUTE = 60_000

export function isSortOrder(value: unknown): value is SortOrder {
  return value === SortOrder.CaptureTime || value === SortOrder.FileName
}

/** Each pair's place in file-name order; pass the pairs as the scan returned them */
export function fileNameRanks(pairs: readonly PhotoPair[]): Map<string, number> {
  return new Map(pairs.map((p, i) => [p.id, i]))
}

/**
 * When each photo was taken, comparable across the folder. Times are compared
 * in UTC only when every photo recorded its time zone: mixing UTC with bare
 * camera clocks would order nothing reliably, so otherwise the camera clocks
 * are compared as they read.
 */
export function captureKeys(pairs: readonly PhotoPair[]): Map<string, number> {
  const timed = pairs.filter((p) => p.captureTime !== null)
  const isUtc = timed.length > 0 && timed.every((p) => p.captureTime?.offsetMinutes != null)
  const keys = new Map<string, number>()
  for (const { id, captureTime } of timed) {
    if (!captureTime) continue
    const offsetMs = isUtc ? (captureTime.offsetMinutes ?? 0) * MS_PER_MINUTE : 0
    keys.set(id, captureTime.localMs - offsetMs)
  }
  return keys
}

/**
 * Pairs in the given order, as a new array. By capture time, photos taken at
 * the same moment keep their file-name order, and photos that don't record a
 * capture time come last, in file-name order.
 */
export function sortPairs(
  pairs: readonly PhotoPair[],
  order: SortOrder,
  ranks: ReadonlyMap<string, number>,
): PhotoPair[] {
  const rank = (p: PhotoPair) => ranks.get(p.id) ?? Number.MAX_SAFE_INTEGER
  const byName = (a: PhotoPair, b: PhotoPair) => rank(a) - rank(b)
  if (order === SortOrder.FileName) return [...pairs].sort(byName)

  const keys = captureKeys(pairs)
  return [...pairs].sort((a, b) => {
    const ka = keys.get(a.id)
    const kb = keys.get(b.id)
    if (ka === undefined || kb === undefined) {
      if (ka !== kb) return ka === undefined ? 1 : -1
      return byName(a, b)
    }
    return ka - kb || byName(a, b)
  })
}

export function countWithoutCaptureTime(pairs: readonly PhotoPair[]): number {
  return pairs.filter((p) => p.captureTime === null).length
}
