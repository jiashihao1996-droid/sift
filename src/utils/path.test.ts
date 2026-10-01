import { describe, expect, it } from 'vitest'
import { extractFileName } from './path'

describe('extractFileName', () => {
  it('handles POSIX paths', () => {
    expect(extractFileName('/Users/me/Photos/DSC_0001.JPG')).toBe('DSC_0001.JPG')
  })

  it('handles Windows paths', () => {
    expect(extractFileName('C:\\Photos\\DSC_0001.ARW')).toBe('DSC_0001.ARW')
  })

  it('returns empty string for a trailing separator', () => {
    expect(extractFileName('/Users/me/Photos/')).toBe('')
  })
})
