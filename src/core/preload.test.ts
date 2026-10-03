import { describe, expect, it } from 'vitest'
import { preloadOrder } from './preload'

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
