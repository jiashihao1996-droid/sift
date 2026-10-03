import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { PhotoStatus, type PhotoPair, type ScanResult } from '@/types'
import { cleanupCache, generateThumbnails, scanFolder } from '@/services/tauriCommands'
import { useSessionStore } from './sessionStore'

vi.mock('@/services/tauriCommands', () => ({
  scanFolder: vi.fn(),
  generateThumbnails: vi.fn(async () => []),
  cleanupCache: vi.fn(async () => {}),
}))

const { Unprocessed: U, Starred: S, Deleted: D, Skipped: K } = PhotoStatus

function makePairs(statuses: PhotoStatus[]): PhotoPair[] {
  return statuses.map((status, i) => ({
    id: `p${i}`,
    jpgPath: `/photos/IMG_${i}.jpg`,
    rawPath: null,
    rawFormat: null,
    status,
    thumbnailPath: null,
    dominantColor: null,
    source: 'jpg',
    xmpPaths: [],
  }))
}

function storeWith(statuses: PhotoStatus[], currentIndex = 0) {
  const store = useSessionStore()
  store.pairs = makePairs(statuses)
  store.currentIndex = currentIndex
  return store
}

const statuses = (store: ReturnType<typeof useSessionStore>) => store.pairs.map((p) => p.status)

beforeEach(() => {
  setActivePinia(createPinia())
  vi.clearAllMocks()
})

describe('counts and progress', () => {
  it('counts each status and computes processed percentage', () => {
    const store = storeWith([U, S, D, K, U, U])

    expect(store.totalCount).toBe(6)
    expect(store.starredCount).toBe(1)
    expect(store.deletedCount).toBe(1)
    expect(store.skippedCount).toBe(1)
    expect(store.unprocessedCount).toBe(3)
    expect(store.progress).toBe(50)
    expect(store.activePairs.map((p) => p.id)).toEqual(['p0', 'p1', 'p3', 'p4', 'p5'])
  })

  it('reports zero progress for an empty session', () => {
    expect(storeWith([]).progress).toBe(0)
  })

  it('has reached the end only when nothing is unprocessed', () => {
    expect(storeWith([S, U]).hasReachedEnd).toBe(false)
    expect(storeWith([S, D, K]).hasReachedEnd).toBe(true)
  })

  it('knows when the current photo is the last one', () => {
    expect(storeWith([U, U], 0).isLastPhoto).toBe(false)
    expect(storeWith([U, U], 1).isLastPhoto).toBe(true)
  })
})

describe('marking', () => {
  it.each([
    ['markStar', S, 'starred', 'star'],
    ['markDelete', D, 'deleted', 'delete'],
    ['markSkip', K, 'skipped', 'skip'],
  ] as const)('%s marks, records undo and advances', (action, status, outcome, type) => {
    const store = storeWith([U, U, U])

    expect(store[action]()).toBe(outcome)

    expect(statuses(store)).toEqual([status, U, U])
    expect(store.undoStack).toEqual([{ type, index: 0, previousStatus: U }])
    expect(store.markActionCount).toBe(1)
    expect(store.currentIndex).toBe(1)
    expect(store.slideDirection).toBe('left')
  })

  it.each([
    ['markStar', S, 'unstarred'],
    ['markDelete', D, 'undeleted'],
    ['markSkip', K, 'unskipped'],
  ] as const)(
    '%s on an already-marked photo reverts it without recording undo',
    (action, status, outcome) => {
      const store = storeWith([status, U])

      expect(store[action]()).toBe(outcome)

      expect(statuses(store)).toEqual([U, U])
      expect(store.undoStack).toEqual([])
      expect(store.markActionCount).toBe(0)
      expect(store.currentIndex).toBe(1)
    },
  )

  it('switching mark type records the previous status', () => {
    const store = storeWith([S, U])
    store.markDelete()
    expect(store.undoStack[0]).toEqual({ type: 'delete', index: 0, previousStatus: S })
  })

  it('skips already-processed photos when advancing', () => {
    const store = storeWith([U, S, D, U])
    store.markStar()
    expect(store.currentIndex).toBe(3)
  })

  it('wraps around to earlier unprocessed photos', () => {
    const store = storeWith([U, S, U], 2)
    store.markStar()

    expect(store.currentIndex).toBe(0)
    // Current behaviour: direction is 'left' even when wrapping backwards
    expect(store.slideDirection).toBe('left')
  })

  it('moves to the next photo when nothing is left unprocessed', () => {
    const store = storeWith([S, U, S])
    store.currentIndex = 1
    store.markStar()
    expect(store.currentIndex).toBe(2)
  })

  it('stays on the last photo when nothing is left unprocessed', () => {
    const store = storeWith([S, U], 1)
    store.slideDirection = 'right'
    store.markStar()

    expect(store.currentIndex).toBe(1)
    expect(store.slideDirection).toBe('right')
  })

  it('is a no-op on an empty session', () => {
    const store = storeWith([])
    expect(store.markStar()).toBe('starred')
    expect(store.undoStack).toEqual([])
  })
})

describe('undo', () => {
  it('restores the previous status and jumps back to that photo', () => {
    const store = storeWith([U, U, U])
    store.markStar()
    store.markDelete()

    store.undo()

    expect(statuses(store)).toEqual([S, U, U])
    expect(store.currentIndex).toBe(1)
    expect(store.slideDirection).toBe('right')
  })

  it('does nothing when there is nothing to undo', () => {
    const store = storeWith([U, U], 1)
    store.undo()
    expect(store.currentIndex).toBe(1)
  })

  it('after a toggle-off, undoes the earlier recorded action', () => {
    // Current behaviour: toggling a mark off is not itself undoable
    const store = storeWith([U, U, U])
    store.markStar() // p0 starred, now on p1
    store.markStar() // p1 starred, now on p2
    store.goTo(0)
    store.markStar() // p0 un-starred (not recorded)

    store.undo()

    expect(statuses(store)).toEqual([U, U, U])
    expect(store.currentIndex).toBe(1)
  })
})

describe('navigation', () => {
  it('moves forward and back within bounds', () => {
    const store = storeWith([U, U])

    store.goPrev()
    expect(store.currentIndex).toBe(0)

    store.goNext()
    expect([store.currentIndex, store.slideDirection]).toEqual([1, 'left'])

    store.goNext()
    expect(store.currentIndex).toBe(1)

    store.goPrev()
    expect([store.currentIndex, store.slideDirection]).toEqual([0, 'right'])
  })

  it('jumps to an index and ignores out-of-range targets', () => {
    const store = storeWith([U, U, U])

    store.goTo(2)
    expect([store.currentIndex, store.slideDirection]).toEqual([2, 'left'])

    store.goTo(5)
    store.goTo(-1)
    expect(store.currentIndex).toBe(2)
  })
})

describe('scanning', () => {
  function scanResultOf(count: number): ScanResult {
    return {
      pairs: makePairs(Array(count).fill(U)),
      totalFiles: count,
      pairedCount: 0,
      jpgOnlyCount: count,
      rawOnlyCount: 0,
    }
  }

  it('loads pairs and resets position and undo history', async () => {
    vi.mocked(scanFolder).mockResolvedValue(scanResultOf(3))
    const store = storeWith([S, S], 1)
    store.undoStack = [{ type: 'star', index: 0, previousStatus: U }]

    await store.startScan('/photos')

    expect(store.folderPath).toBe('/photos')
    expect(store.pairs).toHaveLength(3)
    expect(store.currentIndex).toBe(0)
    expect(store.undoStack).toEqual([])
    expect(store.isScanning).toBe(false)
  })

  it('stops the spinner and rethrows when the scan fails', async () => {
    vi.mocked(scanFolder).mockRejectedValue('Folder does not exist: /nope')
    const store = useSessionStore()

    await expect(store.startScan('/nope')).rejects.toBe('Folder does not exist: /nope')
    expect(store.isScanning).toBe(false)
  })

  it('generates thumbnails in a small first batch, then chunks of 50', async () => {
    vi.mocked(scanFolder).mockResolvedValue(scanResultOf(120))
    vi.mocked(generateThumbnails).mockImplementation(async (inputs) =>
      inputs.map(({ id }) => ({ id, path: `/cache/${id}.jpg`, dominantColor: '#000000' })),
    )
    const store = useSessionStore()

    await store.startScan('/photos')
    await vi.waitFor(() => expect(store.isGeneratingThumbnails).toBe(false))

    const batchSizes = vi.mocked(generateThumbnails).mock.calls.map(([inputs]) => inputs.length)
    expect(batchSizes).toEqual([10, 50, 50, 10])
    expect(store.pairs[119].thumbnailPath).toBe('/cache/p119.jpg')
  })

  it('reset clears the session and cleans the cache', () => {
    const store = storeWith([S, U], 1)
    store.resetSession()

    expect(store.pairs).toEqual([])
    expect(store.currentIndex).toBe(0)
    expect(store.folderPath).toBe('')
    expect(cleanupCache).toHaveBeenCalledOnce()
  })
})
