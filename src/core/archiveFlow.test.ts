import { describe, expect, it, vi } from 'vitest'
import {
  PhotoStatus,
  type DeletePairInput,
  type OrganizeError,
  type PhotoPair,
  type PhotoStatus as Status,
} from '@/types'
import { runArchive, runDeleteOnly, withoutPairs, type ArchiveDeps } from './archiveFlow'

function photo(id: string, status: Status = PhotoStatus.Unprocessed): PhotoPair {
  return {
    id,
    jpgPath: `/photos/${id}.jpg`,
    rawPath: `/photos/${id}.ARW`,
    rawFormat: 'ARW',
    status,
    thumbnailPath: null,
    dominantColor: null,
    source: 'jpg',
    xmpPaths: [],
    captureTime: null,
  }
}

const denied: OrganizeError = {
  kind: 'automationDenied',
  path: '/photos/b.jpg',
  detail: '(-1743)',
  completed: 0,
}

function deps(overrides: Partial<ArchiveDeps> = {}): ArchiveDeps {
  return {
    deletePairs: vi.fn(async (pairs: DeletePairInput[]) => ({
      trashed: pairs.map((p) => p.id),
      failed: [],
    })),
    archivePhotos: vi.fn(async () => ({ movedCount: 3, picksFolder: '', rawFolder: '' })),
    dropPairs: vi.fn(),
    ...overrides,
  }
}

describe('runArchive', () => {
  const pairs = [
    photo('a', PhotoStatus.Deleted),
    photo('b', PhotoStatus.Deleted),
    photo('c', PhotoStatus.Starred),
    photo('d'),
  ]

  it('trashes the deleted pairs in one batch, then archives the rest', async () => {
    const d = deps()

    const outcome = await runArchive('/photos', pairs, d)

    expect(outcome).toEqual({ ok: true, deletedGroups: 2, movedCount: 3 })
    expect(d.deletePairs).toHaveBeenCalledTimes(1)
    expect(vi.mocked(d.deletePairs).mock.calls[0][0].map((p) => p.id)).toEqual(['a', 'b'])
    expect(d.dropPairs).toHaveBeenCalledWith(['a', 'b'])
    const archived = vi.mocked(d.archivePhotos).mock.calls[0][1]
    expect(archived.map((p) => p.jpgPath)).toEqual(['/photos/c.jpg', '/photos/d.jpg'])
  })

  it('skips the delete call when nothing is marked for deletion', async () => {
    const d = deps()

    await runArchive('/photos', [photo('c', PhotoStatus.Starred)], d)

    expect(d.deletePairs).not.toHaveBeenCalled()
  })

  it('drops what was trashed and stops before archiving when a pair fails', async () => {
    const d = deps({
      deletePairs: vi.fn(async () => ({ trashed: ['a'], failed: [{ id: 'b', error: denied }] })),
    })

    const outcome = await runArchive('/photos', pairs, d)

    expect(d.dropPairs).toHaveBeenCalledWith(['a'])
    expect(d.archivePhotos).not.toHaveBeenCalled()
    expect(outcome.ok).toBe(false)
    if (outcome.ok) return
    expect(outcome.report.reason).toContain('访达')
    expect(outcome.report.done).toContain('1 组没有删完，仍标记为删除')
    expect(outcome.report.canRetry).toBe(true)
  })

  it('reports an archive failure with the groups already deleted', async () => {
    const d = deps({
      archivePhotos: vi.fn(async () => {
        throw { kind: 'diskFull', path: '/photos/c.ARW', detail: 'No space', completed: 1 }
      }),
    })

    const outcome = await runArchive('/photos', pairs, d)

    expect(outcome.ok).toBe(false)
    if (outcome.ok) return
    expect(outcome.report.done).toEqual(['已把 2 组照片移到回收站', '已归档 1 张照片'])
    expect(outcome.report.canRetry).toBe(false)
  })

  it('turns a rejected delete call into a retryable failure', async () => {
    const d = deps({
      deletePairs: vi.fn(async () => {
        throw 'Task join error'
      }),
    })

    const outcome = await runArchive('/photos', pairs, d)

    expect(d.dropPairs).not.toHaveBeenCalled()
    expect(outcome.ok).toBe(false)
    if (outcome.ok) return
    expect(outcome.report.detail).toBe('Task join error')
    expect(outcome.report.canRetry).toBe(true)
  })
})

describe('runDeleteOnly', () => {
  const pairs = [photo('a', PhotoStatus.Deleted), photo('c', PhotoStatus.Starred), photo('d')]

  it('deletes the marked pairs and never archives', async () => {
    const d = deps()

    const outcome = await runDeleteOnly(pairs, d)

    expect(outcome).toEqual({ ok: true, deletedGroups: 1 })
    expect(vi.mocked(d.deletePairs).mock.calls[0][0].map((p) => p.id)).toEqual(['a'])
    expect(d.dropPairs).toHaveBeenCalledWith(['a'])
    expect(d.archivePhotos).not.toHaveBeenCalled()
  })

  it('reports a failed pair as a retryable delete-only failure', async () => {
    const d = deps({
      deletePairs: vi.fn(async () => ({ trashed: [], failed: [{ id: 'a', error: denied }] })),
    })

    const outcome = await runDeleteOnly(pairs, d, { isPermanent: true })

    expect(outcome.ok).toBe(false)
    if (outcome.ok) return
    expect(outcome.report.title).toBe('删除没有完成')
    expect(outcome.report.done).toContain('其他照片没有任何改动')
    expect(outcome.report.canRetry).toBe(true)
  })

  it('does nothing when no pair is marked for deletion', async () => {
    const d = deps()

    expect(await runDeleteOnly([photo('d')], d)).toEqual({ ok: true, deletedGroups: 0 })
    expect(d.deletePairs).not.toHaveBeenCalled()
  })
})

describe('withoutPairs', () => {
  const pairs = ['a', 'b', 'c', 'd'].map((id) => photo(id))

  it('keeps the current photo current when it survives', () => {
    const result = withoutPairs(pairs, 2, ['a'])

    expect(result.pairs.map((p) => p.id)).toEqual(['b', 'c', 'd'])
    expect(result.currentIndex).toBe(1)
  })

  it('moves on to the next surviving photo when the current one is dropped', () => {
    expect(withoutPairs(pairs, 1, ['b']).currentIndex).toBe(1)
    expect(withoutPairs(pairs, 3, ['d']).currentIndex).toBe(2)
  })

  it('stays at 0 when nothing is left', () => {
    expect(withoutPairs(pairs, 2, ['a', 'b', 'c', 'd'])).toEqual({ pairs: [], currentIndex: 0 })
  })
})
