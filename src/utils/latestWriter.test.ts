import { describe, expect, it, vi } from 'vitest'
import { createLatestWriter } from './latestWriter'

function deferredWrites() {
  const calls: { value: number; finish: () => void; fail: (e: unknown) => void }[] = []
  const write = vi.fn(
    (value: number) =>
      new Promise<void>((resolve, reject) => {
        calls.push({ value, finish: resolve, fail: reject })
      }),
  )
  return { calls, write }
}

describe('createLatestWriter', () => {
  it('writes a scheduled value right away', async () => {
    const write = vi.fn(async () => {})
    const writer = createLatestWriter(write, vi.fn())

    writer.schedule(1)
    await writer.idle()

    expect(write).toHaveBeenCalledExactlyOnceWith(1)
  })

  it('collapses values scheduled during a write into one write of the newest', async () => {
    const { calls, write } = deferredWrites()
    const writer = createLatestWriter(write, vi.fn())

    writer.schedule(1)
    writer.schedule(2)
    writer.schedule(3)
    calls[0].finish()
    await vi.waitFor(() => expect(calls).toHaveLength(2))
    calls[1].finish()
    await writer.idle()

    expect(calls.map((c) => c.value)).toEqual([1, 3])
  })

  it('reports a failed write and keeps writing later values', async () => {
    const { calls, write } = deferredWrites()
    const onError = vi.fn()
    const writer = createLatestWriter(write, onError)

    writer.schedule(1)
    writer.schedule(2)
    calls[0].fail('disk full')
    await vi.waitFor(() => expect(calls).toHaveLength(2))
    calls[1].finish()
    await writer.idle()

    expect(onError).toHaveBeenCalledExactlyOnceWith('disk full')
    expect(calls.map((c) => c.value)).toEqual([1, 2])
  })

  it('is idle when nothing was scheduled', async () => {
    const writer = createLatestWriter(
      vi.fn(async () => {}),
      vi.fn(),
    )

    await expect(writer.idle()).resolves.toBeUndefined()
  })
})
