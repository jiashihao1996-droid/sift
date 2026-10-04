import { describe, expect, it } from 'vitest'
import { nearestFirst, preloadOrder } from './preload'

describe('preloadOrder', () => {
  it('loads further ahead in the direction of travel, nearest first', () => {
    expect(preloadOrder(5, 20, 1)).toEqual([6, 7, 8, 4])
  })

  it('looks the other way when browsing backwards', () => {
    expect(preloadOrder(5, 20, -1)).toEqual([4, 3, 2, 6])
  })

  it('stays inside the folder', () => {
    expect(preloadOrder(18, 20, 1)).toEqual([19, 17])
    expect(preloadOrder(0, 2, -1)).toEqual([1])
    expect(preloadOrder(0, 1, 1)).toEqual([])
  })
})

describe('nearestFirst', () => {
  const items = ['a', 'b', 'c', 'd', 'e']

  it('starts at the current photo and alternates ahead and behind', () => {
    expect(nearestFirst(items, 2)).toEqual(['c', 'd', 'b', 'e', 'a'])
  })

  it('keeps the folder order when starting at the first photo', () => {
    expect(nearestFirst(items, 0)).toEqual(items)
  })

  it('works backwards from the last photo', () => {
    expect(nearestFirst(items, 4)).toEqual(['e', 'd', 'c', 'b', 'a'])
  })

  it('clamps an index outside the folder and handles an empty one', () => {
    expect(nearestFirst(items, 9)).toEqual(['e', 'd', 'c', 'b', 'a'])
    expect(nearestFirst([], 3)).toEqual([])
  })
})
