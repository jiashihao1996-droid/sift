import { describe, expect, it } from 'vitest'
import {
  addRecentFolder,
  MAX_RECENT_FOLDERS,
  parseRecentFolders,
  removeRecentFolder,
} from './recentFolders'

describe('addRecentFolder', () => {
  it('puts the folder first', () => {
    expect(addRecentFolder(['/a', '/b'], '/c')).toEqual(['/c', '/a', '/b'])
  })

  it('moves an existing folder to the front instead of duplicating it', () => {
    expect(addRecentFolder(['/a', '/b', '/c'], '/b')).toEqual(['/b', '/a', '/c'])
  })

  it('drops the oldest beyond the cap', () => {
    const full = ['/1', '/2', '/3', '/4', '/5']
    expect(addRecentFolder(full, '/6')).toEqual(['/6', '/1', '/2', '/3', '/4'])
  })

  it('does not mutate the input', () => {
    const list = ['/a']
    addRecentFolder(list, '/b')
    expect(list).toEqual(['/a'])
  })
})

describe('removeRecentFolder', () => {
  it('removes the folder', () => {
    expect(removeRecentFolder(['/a', '/b'], '/a')).toEqual(['/b'])
  })
})

describe('parseRecentFolders', () => {
  it('reads a stored list', () => {
    expect(parseRecentFolders('["/a","/b"]')).toEqual(['/a', '/b'])
  })

  it('returns an empty list for missing or malformed data', () => {
    expect(parseRecentFolders(null)).toEqual([])
    expect(parseRecentFolders('not json')).toEqual([])
    expect(parseRecentFolders('{"a":1}')).toEqual([])
  })

  it('drops non-string entries and caps the length', () => {
    const many = JSON.stringify(['/1', 2, '', '/3', '/4', '/5', '/6', '/7'])
    const parsed = parseRecentFolders(many)
    expect(parsed).toEqual(['/1', '/3', '/4', '/5', '/6'])
    expect(parsed).toHaveLength(MAX_RECENT_FOLDERS)
  })
})
