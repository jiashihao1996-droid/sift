import { describe, expect, it } from 'vitest'
import { describeSkipped } from './scanReport'

describe('describeSkipped', () => {
  it('returns nothing when no files were skipped', () => {
    expect(describeSkipped([])).toEqual([])
  })

  it('shows each file by name with a readable reason, keeping the full path', () => {
    const lines = describeSkipped([{ path: '/photos/DSC_1.ARW', reason: 'noPreview' }])

    expect(lines).toEqual([
      { name: 'DSC_1.ARW', path: '/photos/DSC_1.ARW', reason: '无法提取内嵌预览' },
    ])
  })

  it('sorts files naturally by name', () => {
    const lines = describeSkipped([
      { path: 'C:\\raw\\IMG_10.CR3', reason: 'noPreview' },
      { path: '/raw/IMG_2.CR3', reason: 'noPreview' },
    ])

    expect(lines.map((l) => l.name)).toEqual(['IMG_2.CR3', 'IMG_10.CR3'])
  })
})
