// ============================================================
// Sift - Saved progress rules: snapshot marks, merge them into a fresh scan
// Pure — no Vue, Pinia or Tauri imports.
// ============================================================

import { PhotoStatus, type FolderProgress, type PhotoPair } from '@/types'

/**
 * Stable identity of a photo across scans (pair ids are regenerated each scan).
 * RAW-only pairs use the RAW path: their jpgPath is a temporary cache file.
 */
export function progressKey(pair: PhotoPair): string {
  return pair.source === 'rawPreview' && pair.rawPath ? pair.rawPath : pair.jpgPath
}

/**
 * The mark a photo has before the user touches it: a pick already archived
 * into 精选/ is starred, everything else unmarked.
 */
export function defaultStatus(pair: PhotoPair): PhotoStatus {
  return pair.isArchivedPick ? PhotoStatus.Starred : PhotoStatus.Unprocessed
}

/** Saved marks are the ones that differ from the default, unmarking an archived pick included */
export function snapshotProgress(
  folder: string,
  pairs: readonly PhotoPair[],
  currentIndex: number,
): FolderProgress {
  const marks: Record<string, PhotoStatus> = {}
  for (const pair of pairs) {
    if (pair.status !== defaultStatus(pair)) marks[progressKey(pair)] = pair.status
  }
  // The first photo is where a fresh session starts anyway, so it is no progress
  const current = currentIndex > 0 ? pairs[currentIndex] : undefined
  return { folder, currentKey: current ? progressKey(current) : null, marks }
}

/** Progress that restores nothing; saving it forgets the folder */
export function emptyProgress(folder: string): FolderProgress {
  return { folder, currentKey: null, marks: {} }
}

/** Whether reopening the folder would restore anything */
export function hasProgress(progress: FolderProgress): boolean {
  return progress.currentKey !== null || Object.keys(progress.marks).length > 0
}

export interface RestoredProgress {
  pairs: PhotoPair[]
  currentIndex: number
  restoredCount: number
  /** Whether any saved mark or position was applied */
  isResumed: boolean
}

/**
 * Apply saved marks to freshly scanned pairs; a saved mark wins over the
 * default. Marks for files that are gone are dropped; new files keep their
 * default. Resumes at the saved photo, else at the first one.
 */
export function restoreProgress(
  pairs: readonly PhotoPair[],
  saved: FolderProgress | null,
): RestoredProgress {
  if (!saved) return { pairs: [...pairs], currentIndex: 0, restoredCount: 0, isResumed: false }

  let restoredCount = 0
  const restored = pairs.map((pair) => {
    const status = saved.marks[progressKey(pair)]
    if (!status || status === pair.status) return pair
    restoredCount++
    return { ...pair, status }
  })

  const currentIndex = Math.max(
    0,
    restored.findIndex((pair) => progressKey(pair) === saved.currentKey),
  )
  const isResumed = restoredCount > 0 || currentIndex > 0
  return { pairs: restored, currentIndex, restoredCount, isResumed }
}
