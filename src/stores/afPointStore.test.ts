import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useAfPointStore } from './afPointStore'
import { readAfPoint } from '@/services/tauriCommands'
import type { AfInfo, PhotoPair } from '@/types'

vi.mock('@/services/tauriCommands', () => ({ readAfPoint: vi.fn() }))

const FOUND: AfInfo = {
  kind: 'found',
  width: 6000,
  height: 4000,
  orientation: 1,
  area: { x: 10, y: 20, width: 30, height: 40 },
  points: [],
}

function pair(id: string): PhotoPair {
  return {
    id,
    jpgPath: `/photos/${id}.JPG`,
    rawPath: `/photos/${id}.ARW`,
    rawFormat: 'ARW',
    status: 'unprocessed',
    thumbnailPath: null,
    dominantColor: null,
    source: 'jpg',
    xmpPaths: [],
    captureTime: null,
    isArchivedPick: false,
  }
}

/** Let the mocked reads settle */
function flushPromises() {
  return new Promise((resolve) => setTimeout(resolve))
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.mocked(readAfPoint).mockReset()
})

describe('afPointStore', () => {
  it('reads each photo once and serves it afterwards', async () => {
    vi.mocked(readAfPoint).mockResolvedValue(FOUND)
    const store = useAfPointStore()
    const a = pair('a')

    store.load([a, a])
    expect(store.infoOf(a)).toBeUndefined()
    await flushPromises()
    store.load([a])

    expect(store.infoOf(a)).toEqual(FOUND)
    expect(readAfPoint).toHaveBeenCalledTimes(1)
    expect(readAfPoint).toHaveBeenCalledWith('/photos/a.JPG', '/photos/a.ARW', 'jpg')
  })

  it('treats a failed read as no focus data', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => {})
    vi.mocked(readAfPoint).mockRejectedValue('Task join error')
    const store = useAfPointStore()
    const a = pair('a')

    store.load([a])
    await flushPromises()

    expect(store.infoOf(a)).toEqual({ kind: 'unavailable' })
  })

  it('skips missing pairs', () => {
    const store = useAfPointStore()
    store.load([null, undefined])
    expect(readAfPoint).not.toHaveBeenCalled()
    expect(store.infoOf(null)).toBeUndefined()
  })
})
