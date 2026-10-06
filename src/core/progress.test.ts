import { describe, expect, it } from 'vitest'
import { PhotoStatus, type FolderProgress, type PhotoPair } from '@/types'
import {
  defaultStatus,
  emptyProgress,
  hasProgress,
  progressKey,
  restoreProgress,
  snapshotProgress,
} from './progress'

const { Unprocessed: U, Starred: S, Deleted: D } = PhotoStatus

function jpg(name: string, status: PhotoStatus = U): PhotoPair {
  return {
    id: `id-${name}-${Math.random()}`,
    jpgPath: `/photos/${name}.jpg`,
    rawPath: `/photos/${name}.CR3`,
    rawFormat: 'CR3',
    status,
    thumbnailPath: null,
    dominantColor: null,
    source: 'jpg',
    xmpPaths: [],
    captureTime: null,
    isArchivedPick: false,
  }
}

/** A pick a previous archive moved into 精选/: starred unless changed */
function archivedPick(name: string, status: PhotoStatus = S): PhotoPair {
  return {
    ...jpg(name, status),
    jpgPath: `/photos/精选/${name}.jpg`,
    rawPath: `/photos/精选/${name}.CR3`,
    isArchivedPick: true,
  }
}

function rawOnly(name: string, status: PhotoStatus = U): PhotoPair {
  return {
    ...jpg(name, status),
    jpgPath: `/tmp/sift-raw-previews/${Math.random()}.jpg`,
    source: 'rawPreview',
  }
}

function saved(marks: Record<string, PhotoStatus>, currentKey: string | null = null) {
  const progress: FolderProgress = { folder: '/photos', currentKey, marks }
  return progress
}

describe('progressKey', () => {
  it('uses the JPG path for normal pairs', () => {
    expect(progressKey(jpg('IMG_1'))).toBe('/photos/IMG_1.jpg')
  })

  it('uses the RAW path for RAW-only pairs, whose JPG is a cache file', () => {
    expect(progressKey(rawOnly('IMG_1'))).toBe('/photos/IMG_1.CR3')
  })
})

describe('defaultStatus', () => {
  it('starts archived picks starred and everything else unmarked', () => {
    expect(defaultStatus(archivedPick('A'))).toBe(S)
    expect(defaultStatus(jpg('B'))).toBe(U)
  })
})

describe('snapshotProgress', () => {
  it('records archived picks only when they no longer are starred', () => {
    const pairs = [archivedPick('A'), archivedPick('B', U), archivedPick('C', D)]

    expect(snapshotProgress('/photos', pairs, 0).marks).toEqual({
      '/photos/精选/B.jpg': U,
      '/photos/精选/C.jpg': D,
    })
  })

  it('saves nothing for a folder whose archived picks are untouched', () => {
    const progress = snapshotProgress('/photos', [archivedPick('A'), jpg('B')], 0)

    expect(hasProgress(progress)).toBe(false)
  })

  it('records only marked photos and the current photo', () => {
    const pairs = [jpg('A', S), jpg('B'), rawOnly('C', D)]

    expect(snapshotProgress('/photos', pairs, 1)).toEqual({
      folder: '/photos',
      currentKey: '/photos/B.jpg',
      marks: { '/photos/A.jpg': S, '/photos/C.CR3': D },
    })
  })

  it('records no position on the first photo, where a fresh session starts anyway', () => {
    expect(snapshotProgress('/photos', [jpg('A'), jpg('B')], 0).currentKey).toBeNull()
  })

  it('has no current photo for an empty session', () => {
    expect(snapshotProgress('/photos', [], 0).currentKey).toBeNull()
  })
})

describe('hasProgress', () => {
  it('is true once something is marked or the position moved past the first photo', () => {
    expect(hasProgress(saved({ '/photos/A.jpg': S }))).toBe(true)
    expect(hasProgress(saved({}, '/photos/B.jpg'))).toBe(true)
  })

  it('is false for an untouched folder', () => {
    expect(hasProgress(saved({}))).toBe(false)
    expect(hasProgress(emptyProgress('/photos'))).toBe(false)
  })
})

describe('restoreProgress', () => {
  it('leaves pairs untouched and starts at the first photo without saved progress', () => {
    const pairs = [jpg('A'), jpg('B')]

    const result = restoreProgress(pairs, null)

    expect(result).toEqual({ pairs, currentIndex: 0, restoredCount: 0, isResumed: false })
  })

  it('reapplies saved marks by file path even though pair ids changed', () => {
    const pairs = [jpg('A'), jpg('B'), rawOnly('C')]

    const result = restoreProgress(
      pairs,
      saved({ '/photos/A.jpg': S, '/photos/C.CR3': D }, '/photos/B.jpg'),
    )

    expect(result.pairs.map((p) => p.status)).toEqual([S, U, D])
    expect(result.pairs[0].id).toBe(pairs[0].id)
    expect(result.restoredCount).toBe(2)
    expect(result.currentIndex).toBe(1)
    expect(result.isResumed).toBe(true)
  })

  it('resumes at a saved position even when nothing was marked', () => {
    const result = restoreProgress([jpg('A'), jpg('B')], saved({}, '/photos/B.jpg'))

    expect(result.currentIndex).toBe(1)
    expect(result.restoredCount).toBe(0)
    expect(result.isResumed).toBe(true)
  })

  it('drops marks for files that disappeared and keeps new files unprocessed', () => {
    const pairs = [jpg('A'), jpg('NEW')]

    const result = restoreProgress(pairs, saved({ '/photos/A.jpg': D, '/photos/GONE.jpg': S }))

    expect(result.pairs.map((p) => p.status)).toEqual([D, U])
    expect(result.restoredCount).toBe(1)
  })

  it('starts at the first photo when the saved one is gone', () => {
    const pairs = [jpg('A'), jpg('B'), jpg('C')]

    const result = restoreProgress(
      pairs,
      saved({ '/photos/A.jpg': S, '/photos/B.jpg': D }, '/photos/GONE.jpg'),
    )

    expect(result.currentIndex).toBe(0)
    expect(result.isResumed).toBe(true)
  })

  it('is not resumed when nothing saved still applies', () => {
    const result = restoreProgress([jpg('A')], saved({ '/photos/GONE.jpg': S }, '/photos/GONE.jpg'))

    expect(result.isResumed).toBe(false)
  })

  it('restores an archived pick that was unmarked or marked for deletion', () => {
    const pairs = [archivedPick('A'), archivedPick('B'), archivedPick('C')]

    const result = restoreProgress(
      pairs,
      saved({ '/photos/精选/A.jpg': U, '/photos/精选/B.jpg': D }),
    )

    expect(result.pairs.map((p) => p.status)).toEqual([U, D, S])
    expect(result.restoredCount).toBe(2)
    expect(result.isResumed).toBe(true)
  })

  it('keeps archived picks starred when their marks were not saved', () => {
    const result = restoreProgress([archivedPick('A')], saved({ '/photos/B.jpg': D }))

    expect(result.pairs[0].status).toBe(S)
    expect(result.isResumed).toBe(false)
  })

  it('round-trips an unmarked archived pick through a snapshot', () => {
    const marked = [archivedPick('A', U), jpg('B', S)]
    const scanned = [archivedPick('A'), jpg('B')]

    const result = restoreProgress(scanned, snapshotProgress('/photos', marked, 0))

    expect(result.pairs.map((p) => p.status)).toEqual([U, S])
  })

  it('does not mutate the scanned pairs', () => {
    const pairs = [jpg('A')]

    restoreProgress(pairs, saved({ '/photos/A.jpg': S }))

    expect(pairs[0].status).toBe(U)
  })
})
