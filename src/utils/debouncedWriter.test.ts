import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createDebouncedWriter } from './debouncedWriter'
import { createLatestWriter } from './latestWriter'

const TIMING = { delayMs: 500, maxWaitMs: 2000 }

function setup() {
  const written: number[] = []
  const write = vi.fn(async (value: number) => {
    written.push(value)
  })
  const writer = createDebouncedWriter(createLatestWriter(write, vi.fn()), TIMING)
  return { writer, written, write }
}

describe('createDebouncedWriter', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('writes once after changes settle, with the latest value', async () => {
    const { writer, written } = setup()
    let value = 0
    for (let i = 0; i < 10; i++) {
      value = i
      writer.schedule(() => value)
      await vi.advanceTimersByTimeAsync(100)
    }
    expect(written).toEqual([])

    await vi.advanceTimersByTimeAsync(500)

    expect(written).toEqual([9])
  })

  it('takes the snapshot when writing, not when scheduling', async () => {
    const { writer } = setup()
    const snapshot = vi.fn(() => 1)
    for (let i = 0; i < 5; i++) writer.schedule(snapshot)
    expect(snapshot).not.toHaveBeenCalled()

    await vi.advanceTimersByTimeAsync(500)

    expect(snapshot).toHaveBeenCalledOnce()
  })

  it('still writes every so often while changes keep coming', async () => {
    const { writer, written } = setup()
    let value = 0
    // A held arrow key: a change every 50ms for 5 seconds
    for (let t = 0; t < 5000; t += 50) {
      value = t
      writer.schedule(() => value)
      await vi.advanceTimersByTimeAsync(50)
    }

    expect(written.length).toBe(2)
    await vi.advanceTimersByTimeAsync(500)
    expect(written.length).toBe(3)
    expect(written.at(-1)).toBe(4950)
  })

  it('flush writes the pending value at once and resolves after it is written', async () => {
    const { writer, written } = setup()
    writer.schedule(() => 7)

    await writer.flush()

    expect(written).toEqual([7])
    await vi.advanceTimersByTimeAsync(1000)
    expect(written).toEqual([7])
  })

  it('flush with nothing pending writes nothing', async () => {
    const { writer, write } = setup()
    await writer.flush()
    expect(write).not.toHaveBeenCalled()
  })

  it('writeNow replaces the pending value', async () => {
    const { writer, written } = setup()
    writer.schedule(() => 1)

    writer.writeNow(2)
    await vi.advanceTimersByTimeAsync(1000)

    expect(written).toEqual([2])
  })

  it('a change after a write starts a new wait', async () => {
    const { writer, written } = setup()
    writer.schedule(() => 1)
    await vi.advanceTimersByTimeAsync(500)
    writer.schedule(() => 2)
    await vi.advanceTimersByTimeAsync(499)
    expect(written).toEqual([1])

    await vi.advanceTimersByTimeAsync(1)

    expect(written).toEqual([1, 2])
  })
})
