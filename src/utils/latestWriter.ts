// ============================================================
// Sift - Latest-value writer
// Runs one async write at a time; values scheduled meanwhile collapse
// into a single follow-up write, so the newest value always lands last.
// ============================================================

export interface LatestWriter<T> {
  schedule(value: T): void
  /** Resolves once every scheduled value has been written */
  idle(): Promise<void>
}

export function createLatestWriter<T>(
  write: (value: T) => Promise<void>,
  onError: (error: unknown) => void,
): LatestWriter<T> {
  let pending: { value: T } | null = null
  let running: Promise<void> | null = null

  async function drain() {
    while (pending) {
      const { value } = pending
      pending = null
      try {
        await write(value)
      } catch (e) {
        onError(e)
      }
    }
    running = null
  }

  return {
    schedule(value) {
      pending = { value }
      running ??= drain()
    },
    idle: () => running ?? Promise.resolve(),
  }
}
