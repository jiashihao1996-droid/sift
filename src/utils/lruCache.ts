// ============================================================
// Sift - Least-recently-used cache
// A Map keeps insertion order; re-inserting on access makes it recency order.
// ============================================================

export class LruCache<K, V> {
  private readonly entries = new Map<K, V>()

  constructor(private readonly capacity: number) {}

  has(key: K): boolean {
    return this.entries.has(key)
  }

  /** The value for `key`, marking it as most recently used */
  get(key: K): V | undefined {
    if (!this.entries.has(key)) return undefined
    const value = this.entries.get(key) as V
    this.entries.delete(key)
    this.entries.set(key, value)
    return value
  }

  set(key: K, value: V): void {
    this.entries.delete(key)
    this.entries.set(key, value)
    if (this.entries.size > this.capacity) {
      const oldest = this.entries.keys().next().value
      if (oldest !== undefined) this.entries.delete(oldest)
    }
  }

  delete(key: K): void {
    this.entries.delete(key)
  }
}
