// ============================================================
// Sift - Saved progress rules: snapshot marks, merge them into a fresh scan
// Pure — no Vue, Pinia or Tauri imports.
// ============================================================

import { PhotoStatus, type FolderProgress, type PhotoPair } from '@/types'
import { findNextUnprocessed } from './review'

/**
 * Stable identity of a photo across scans (pair ids are regenerated each scan).
 * RAW-only pairs use the RAW path: their jpgPath is a temporary cache file.
 */
export function progressKey(pair: PhotoPair): string {
  return pair.source === 'rawPreview' && pair.rawPath ? pair.rawPath : pair.jpgPath
}

export function snapshotProgress(
  folder: string,
  pairs: readonly PhotoPair[],
  currentIndex: number,
): FolderProgress {
  const marks: Record<string, PhotoStatus> = {}
  for (const pair of pairs) {
    if (pair.status !== PhotoStatus.Unprocessed) marks[progressKey(pair)] = pair.status
  }
  const current = pairs[currentIndex]
  return { folder, currentKey: current ? progressKey(current) : null, marks }
}

export function hasMarks(progress: FolderProgress): boolean {
  return Object.keys(progress.marks).length > 0
}

export interface RestoredProgress {
  pairs: PhotoPair[]
  currentIndex: number
  restoredCount: number
}

/**
 * Apply saved marks to freshly scanned pairs. Marks for files that are gone
 * are dropped; new files stay unprocessed. Resumes at the saved photo, else
 * at the first unprocessed one.
 */
export function restoreProgress(
  pairs: readonly PhotoPair[],
  saved: FolderProgress | null,
): RestoredProgress {
  if (!saved) return { pairs: [...pairs], currentIndex: 0, restoredCount: 0 }

  let restoredCount = 0
  const restored = pairs.map((pair) => {
    const status = saved.marks[progressKey(pair)]
    if (!status || status === PhotoStatus.Unprocessed) return pair
    restoredCount++
    return { ...pair, status }
  })

  const savedIndex = restored.findIndex((pair) => progressKey(pair) === saved.currentKey)
  const currentIndex = savedIndex >= 0 ? savedIndex : (findNextUnprocessed(restored, -1) ?? 0)
  return { pairs: restored, currentIndex, restoredCount }
}
