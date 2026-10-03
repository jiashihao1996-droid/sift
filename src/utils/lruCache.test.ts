import { describe, expect, it } from 'vitest'
import { LruCache } from './lruCache'

describe('LruCache', () => {
  it('evicts the least recently used entry when full', () => {
    const cache = new LruCache<string, number>(2)
    cache.set('a', 1)
    cache.set('b', 2)
    cache.get('a') // a is now the most recent
    cache.set('c', 3)

    expect(cache.has('a')).toBe(true)
    expect(cache.has('b')).toBe(false)
    expect(cache.get('c')).toBe(3)
  })

  it('updates an existing key without evicting another', () => {
    const cache = new LruCache<string, number>(2)
    cache.set('a', 1)
    cache.set('b', 2)
    cache.set('a', 10)

    expect([cache.get('a'), cache.get('b')]).toEqual([10, 2])
  })

  it('forgets a deleted key', () => {
    const cache = new LruCache<string, number>(2)
    cache.set('a', 1)
    cache.delete('a')

    expect(cache.get('a')).toBeUndefined()
  })
})
