import { beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'
import { createPinia, setActivePinia } from 'pinia'
import { PhotoStatus, type PhotoPair, type ScanResult } from '@/types'
import {
  cleanupCache,
  clearProgress,
  generateThumbnails,
  loadProgress,
  saveProgress,
  scanFolder,
} from '@/services/tauriCommands'
import { useSessionStore } from './sessionStore'
import { useViewStore } from './viewStore'

vi.mock('@/services/tauriCommands', () => ({
  scanFolder: vi.fn(),
  generateThumbnails: vi.fn(async () => []),
  cleanupCache: vi.fn(async () => {}),
  loadProgress: vi.fn(async () => null),
  saveProgress: vi.fn(async () => {}),
  clearProgress: vi.fn(async () => {}),
}))

const { Unprocessed: U, Starred: S, Deleted: D } = PhotoStatus

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
  // The view store reads viewer preferences; keep tests off the real storage
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {} })
  setActivePinia(createPinia())
  vi.resetAllMocks() // also undoes per-test mockResolvedValue overrides
  vi.restoreAllMocks()
})

function scanResultOf(count: number): ScanResult {
  return {
    pairs: makePairs(Array(count).fill(U)),
    totalFiles: count,
    pairedCount: 0,
    jpgOnlyCount: count,
    rawOnlyCount: 0,
    skipped: [],
  }
}

describe('counts and progress', () => {
  it('counts each status and reports how far through the folder the current photo is', () => {
    const store = storeWith([U, S, D, U], 1)

    expect(store.totalCount).toBe(4)
    expect(store.starredCount).toBe(1)
    expect(store.deletedCount).toBe(1)
    expect(store.unprocessedCount).toBe(2)
    expect(store.progress).toBe(50)
  })

  it('reports zero progress for an empty session', () => {
    expect(storeWith([]).progress).toBe(0)
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
  ] as const)('%s marks, records undo and advances', (action, status, outcome, type) => {
    const store = storeWith([U, U, U])

    expect(store[action]()).toBe(outcome)

    expect(statuses(store)).toEqual([status, U, U])
    expect(store.undoStack).toEqual([{ type, index: 0, previousStatus: U }])
    expect(store.currentIndex).toBe(1)
  })

  it.each([
    ['markStar', S, 'unstarred', 'star'],
    ['markDelete', D, 'undeleted', 'delete'],
  ] as const)(
    '%s on an already-marked photo unmarks it, records undo and stays',
    (action, status, outcome, type) => {
      const store = storeWith([U, status, U], 1)

      expect(store[action]()).toBe(outcome)

      expect(statuses(store)).toEqual([U, U, U])
      expect(store.undoStack).toEqual([{ type, index: 1, previousStatus: status }])
      expect(store.currentIndex).toBe(1)
    },
  )

  it('switching mark type records the previous status and advances', () => {
    const store = storeWith([S, U])
    store.markDelete()

    expect(store.undoStack[0]).toEqual({ type: 'delete', index: 0, previousStatus: S })
    expect(store.currentIndex).toBe(1)
  })

  it('advances to the next photo even when it is already marked', () => {
    const store = storeWith([U, S, U])
    store.markStar()
    expect(store.currentIndex).toBe(1)
  })

  it('stays on the last photo and reports reaching the end', () => {
    const store = storeWith([U, U], 1)
    store.markStar()

    expect(store.currentIndex).toBe(1)
    expect(store.endReachedCount).toBe(1)
  })

  it('does not report the end when unmarking the last photo', () => {
    const store = storeWith([U, S], 1)
    store.markStar()
    expect(store.endReachedCount).toBe(0)
  })

  it('records each mark change for feedback, counting repeats separately', () => {
    const store = storeWith([U, S, U])

    store.markStar()
    const first = store.lastMark
    store.markStar()

    expect(first).toMatchObject({ type: 'star', isMarked: true, index: 0 })
    expect(store.lastMark).toMatchObject({ type: 'star', isMarked: false, index: 1 })
    expect(store.lastMark?.seq).toBe((first?.seq ?? 0) + 1)
  })

  it('does not report undo as a mark change', () => {
    const store = storeWith([U, U])
    store.markDelete()
    const marked = store.lastMark

    store.undo()

    expect(store.lastMark).toBe(marked)
  })

  it('is a no-op on an empty session', () => {
    const store = storeWith([])
    expect(store.markStar()).toBe('starred')
    expect(store.undoStack).toEqual([])
    expect(store.lastMark).toBeNull()
  })
})

describe('marking with stay-after-mark on', () => {
  function stayingStoreWith(statuses: PhotoStatus[], currentIndex = 0) {
    useViewStore().stayAfterMark = true
    return storeWith(statuses, currentIndex)
  }

  it('stays on the marked photo so a burst can be compared in place', () => {
    const store = stayingStoreWith([U, U, U], 1)

    store.markStar()
    expect([statuses(store), store.currentIndex]).toEqual([[U, S, U], 1])

    store.markDelete()
    expect([statuses(store), store.currentIndex]).toEqual([[U, D, U], 1])
  })

  it('does not report reaching the end when marking the last photo', () => {
    const store = stayingStoreWith([U, U], 1)
    store.markStar()
    expect(store.endReachedCount).toBe(0)
  })

  it('still records every change for undo', () => {
    const store = stayingStoreWith([U, U])
    store.markStar()
    store.markStar()

    expect(store.undoStack).toHaveLength(2)
    store.undo()
    expect([statuses(store), store.currentIndex]).toEqual([[S, U], 0])
  })
})

describe('undo', () => {
  it('restores the previous status and jumps back to that photo', () => {
    const store = storeWith([U, U, U])
    store.markStar()
    store.markDelete()

    expect(store.undo()).toBe(true)

    expect(statuses(store)).toEqual([S, U, U])
    expect(store.currentIndex).toBe(1)
  })

  it('reports when there is nothing to undo', () => {
    const store = storeWith([U, U], 1)

    expect(store.undo()).toBe(false)
    expect(store.currentIndex).toBe(1)
  })

  it('undoes an unmark first, bringing the mark back', () => {
    const store = storeWith([U, U, U])
    store.markStar() // p0 starred, now on p1
    store.markStar() // p1 starred, now on p2
    store.goTo(0)
    store.markStar() // p0 unmarked, stays on p0

    store.undo()

    expect(statuses(store)).toEqual([S, S, U])
    expect(store.currentIndex).toBe(0)

    store.undo()

    expect(statuses(store)).toEqual([S, U, U])
    expect(store.currentIndex).toBe(1)
  })
})

describe('navigation', () => {
  it('moves forward and back within bounds', () => {
    const store = storeWith([U, U])

    store.goPrev()
    expect(store.currentIndex).toBe(0)

    store.goNext()
    expect(store.currentIndex).toBe(1)

    store.goPrev()
    expect(store.currentIndex).toBe(0)
  })

  it('never changes marks', () => {
    const store = storeWith([S, D, U])

    store.goNext()
    store.goNext()
    store.goPrev()

    expect(statuses(store)).toEqual([S, D, U])
  })

  it('reports reaching the end when moving on from the last photo', () => {
    const store = storeWith([U, U], 1)

    store.goNext()

    expect(store.currentIndex).toBe(1)
    expect(store.endReachedCount).toBe(1)
  })

  it('jumps to an index and ignores out-of-range targets', () => {
    const store = storeWith([U, U, U])

    store.goTo(2)
    expect(store.currentIndex).toBe(2)

    store.goTo(5)
    store.goTo(-1)
    expect(store.currentIndex).toBe(2)
  })
})

describe('scanning', () => {
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

describe('saved progress', () => {
  async function scannedStore(count: number) {
    vi.mocked(scanFolder).mockResolvedValue(scanResultOf(count))
    const store = useSessionStore()
    await store.startScan('/photos')
    await settle(store)
    vi.mocked(saveProgress).mockClear()
    vi.mocked(clearProgress).mockClear()
    return store
  }

  async function settle(store: ReturnType<typeof useSessionStore>) {
    await nextTick()
    await store.progressSaved()
  }

  it('restores saved marks and resumes at the saved photo when a folder is reopened', async () => {
    vi.mocked(scanFolder).mockResolvedValue(scanResultOf(4))
    vi.mocked(loadProgress).mockResolvedValue({
      folder: '/photos',
      currentKey: '/photos/IMG_2.jpg',
      marks: { '/photos/IMG_0.jpg': S, '/photos/IMG_1.jpg': D },
    })
    const store = useSessionStore()

    await store.startScan('/photos')

    expect(loadProgress).toHaveBeenCalledWith('/photos')
    expect(statuses(store)).toEqual([S, D, U, U])
    expect(store.currentIndex).toBe(2)
    expect(store.restoredCount).toBe(2)
    expect(store.isResumed).toBe(true)
  })

  it('starts fresh when saved progress cannot be read', async () => {
    vi.mocked(scanFolder).mockResolvedValue(scanResultOf(2))
    vi.mocked(loadProgress).mockRejectedValue('permission denied')
    vi.spyOn(console, 'warn').mockImplementation(() => {})
    const store = useSessionStore()

    await store.startScan('/photos')

    expect(statuses(store)).toEqual([U, U])
    expect(store.restoredCount).toBe(0)
  })

  it('saves marks and position after each mark', async () => {
    const store = await scannedStore(3)

    store.markStar()
    await settle(store)

    expect(saveProgress).toHaveBeenLastCalledWith({
      folder: '/photos',
      currentKey: '/photos/IMG_1.jpg',
      marks: { '/photos/IMG_0.jpg': S },
    })
  })

  it('saves the position even when nothing is marked', async () => {
    const store = await scannedStore(3)

    store.goNext()
    await settle(store)

    expect(saveProgress).toHaveBeenLastCalledWith({
      folder: '/photos',
      currentKey: '/photos/IMG_1.jpg',
      marks: {},
    })
  })

  it('saves the mark again when an unmark is undone', async () => {
    const store = await scannedStore(3)
    store.goNext()
    store.markStar() // p1 starred, now on p2
    store.goPrev()
    store.markStar() // p1 unmarked
    await settle(store)

    store.undo()
    await settle(store)

    expect(saveProgress).toHaveBeenLastCalledWith({
      folder: '/photos',
      currentKey: '/photos/IMG_1.jpg',
      marks: { '/photos/IMG_1.jpg': S },
    })
  })

  it('clears saved progress once the last mark is undone', async () => {
    const store = await scannedStore(2)
    store.markDelete()
    await settle(store)

    store.undo()
    await settle(store)

    expect(clearProgress).toHaveBeenLastCalledWith('/photos')
  })

  it('restart drops restored marks and forgets the saved progress', async () => {
    vi.mocked(loadProgress).mockResolvedValue({
      folder: '/photos',
      currentKey: null,
      marks: { '/photos/IMG_0.jpg': S },
    })
    const store = await scannedStore(2)

    store.restartSession()
    await settle(store)

    expect(statuses(store)).toEqual([U, U])
    expect(store.restoredCount).toBe(0)
    expect(store.isResumed).toBe(false)
    expect(clearProgress).toHaveBeenCalledWith('/photos')
  })

  it('forgets saved progress on request, e.g. after archiving', async () => {
    const store = await scannedStore(2)
    store.markStar()
    await settle(store)

    store.forgetProgress()
    await settle(store)

    expect(clearProgress).toHaveBeenLastCalledWith('/photos')
  })

  it('dropping trashed pairs keeps the current photo and clears undo', async () => {
    const store = await scannedStore(3)
    store.goTo(2)
    store.markDelete()
    store.goTo(2)
    await settle(store)
    vi.mocked(saveProgress).mockClear()

    store.dropPairs(['p0'])
    await settle(store)

    expect(store.pairs.map((p) => p.id)).toEqual(['p1', 'p2'])
    expect(store.currentPair?.id).toBe('p2')
    expect(store.undoStack).toEqual([])
    expect(saveProgress).toHaveBeenCalled()
  })

  it('does not save anything when the session is reset', async () => {
    const store = await scannedStore(2)

    store.resetSession()
    await settle(store)

    expect(saveProgress).not.toHaveBeenCalled()
    expect(clearProgress).not.toHaveBeenCalled()
  })
})
