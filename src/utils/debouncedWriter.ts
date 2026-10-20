// ============================================================
// Sift - Debounced writer
// Waits for changes to settle before writing, so a burst of key presses
// costs one write instead of one per key. The value is taken when the
// write happens, not when it is scheduled, so a burst also costs only one
// snapshot. Writes go through a LatestWriter: one at a time, newest last.
// ============================================================

import type { LatestWriter } from './latestWriter'

export interface DebouncedWriter<T> {
  /** Write `snapshot()` once no change has come for a while */
  schedule(snapshot: () => T): void
  /** Write the pending snapshot now; resolves once every write so far is done */
  flush(): Promise<void>
  /** Write `value` now, dropping the pending snapshot */
  writeNow(value: T): void
}

export interface DebounceTiming {
  /** Quiet time after the last change before writing */
  delayMs: number
  /** Longest a change waits while changes keep coming */
  maxWaitMs: number
}

export function createDebouncedWriter<T>(
  writer: LatestWriter<T>,
  { delayMs, maxWaitMs }: DebounceTiming,
): DebouncedWriter<T> {
  let pending: (() => T) | null = null
  let firstScheduledAt = 0
  let timer: ReturnType<typeof setTimeout> | null = null

  function stopTimer() {
    if (timer !== null) clearTimeout(timer)
    timer = null
  }

  function writePending() {
    stopTimer()
    const snapshot = pending
    pending = null
    if (snapshot) writer.schedule(snapshot())
  }

  return {
    schedule(snapshot) {
      const now = Date.now()
      if (!pending) firstScheduledAt = now
      pending = snapshot
      stopTimer()
      const untilMaxWait = firstScheduledAt + maxWaitMs - now
      timer = setTimeout(writePending, Math.max(0, Math.min(delayMs, untilMaxWait)))
    },
    flush() {
      writePending()
      return writer.idle()
    },
    writeNow(value) {
      stopTimer()
      pending = null
      writer.schedule(value)
    },
  }
}
