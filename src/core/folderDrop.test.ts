import { describe, expect, it } from 'vitest'
import { parentDir, resolveDropTarget } from './folderDrop'

describe('resolveDropTarget', () => {
  it('returns a dropped folder as-is', () => {
    expect(resolveDropTarget(['/Users/me/Photos/2026-10-01'])).toBe('/Users/me/Photos/2026-10-01')
  })

  it('opens the folder containing a dropped photo', () => {
    expect(resolveDropTarget(['/Users/me/Photos/trip/DSC_0001.ARW'])).toBe('/Users/me/Photos/trip')
    expect(resolveDropTarget(['C:\\Photos\\trip\\IMG_1.JPG'])).toBe('C:\\Photos\\trip')
  })

  it('treats dotted folder names as folders', () => {
    expect(resolveDropTarget(['/Users/me/Photos/2026.10.01'])).toBe('/Users/me/Photos/2026.10.01')
  })

  it('uses the first of several dropped paths', () => {
    expect(resolveDropTarget(['/a/b/one.jpg', '/c/d/two.jpg'])).toBe('/a/b')
  })

  it('returns null for an empty drop', () => {
    expect(resolveDropTarget([])).toBeNull()
  })
})

describe('parentDir', () => {
  it('keeps the root separator', () => {
    expect(parentDir('/photo.jpg')).toBe('/')
    expect(parentDir('C:\\photo.jpg')).toBe('C:\\')
  })

  it('ignores a trailing separator', () => {
    expect(parentDir('/a/b/')).toBe('/a')
  })
})
