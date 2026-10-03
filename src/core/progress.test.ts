import { describe, expect, it } from 'vitest'
import { PhotoStatus, type FolderProgress, type PhotoPair } from '@/types'
import { hasMarks, progressKey, restoreProgress, snapshotProgress } from './progress'

const { Unprocessed: U, Starred: S, Deleted: D, Skipped: K } = PhotoStatus

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

describe('snapshotProgress', () => {
  it('records only marked photos and the current photo', () => {
    const pairs = [jpg('A', S), jpg('B'), rawOnly('C', D), jpg('D', K)]

    expect(snapshotProgress('/photos', pairs, 1)).toEqual({
      folder: '/photos',
      currentKey: '/photos/B.jpg',
      marks: { '/photos/A.jpg': S, '/photos/C.CR3': D, '/photos/D.jpg': K },
    })
  })

  it('has no current photo for an empty session', () => {
    expect(snapshotProgress('/photos', [], 0).currentKey).toBeNull()
  })
})

describe('hasMarks', () => {
  it('is false when nothing is marked', () => {
    expect(hasMarks(saved({}))).toBe(false)
    expect(hasMarks(saved({ '/photos/A.jpg': S }))).toBe(true)
  })
})

describe('restoreProgress', () => {
  it('leaves pairs untouched and starts at the first photo without saved progress', () => {
    const pairs = [jpg('A'), jpg('B')]

    const result = restoreProgress(pairs, null)

    expect(result).toEqual({ pairs, currentIndex: 0, restoredCount: 0 })
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
  })

  it('drops marks for files that disappeared and keeps new files unprocessed', () => {
    const pairs = [jpg('A'), jpg('NEW')]

    const result = restoreProgress(pairs, saved({ '/photos/A.jpg': K, '/photos/GONE.jpg': S }))

    expect(result.pairs.map((p) => p.status)).toEqual([K, U])
    expect(result.restoredCount).toBe(1)
  })

  it('resumes at the first unprocessed photo when the saved one is gone', () => {
    const pairs = [jpg('A'), jpg('B'), jpg('C')]

    const result = restoreProgress(
      pairs,
      saved({ '/photos/A.jpg': S, '/photos/B.jpg': D }, '/photos/GONE.jpg'),
    )

    expect(result.currentIndex).toBe(2)
  })

  it('falls back to the first photo when everything is marked', () => {
    const pairs = [jpg('A'), jpg('B')]

    const result = restoreProgress(pairs, saved({ '/photos/A.jpg': S, '/photos/B.jpg': S }))

    expect(result.currentIndex).toBe(0)
  })

  it('does not mutate the scanned pairs', () => {
    const pairs = [jpg('A')]

    restoreProgress(pairs, saved({ '/photos/A.jpg': S }))

    expect(pairs[0].status).toBe(U)
  })
})
