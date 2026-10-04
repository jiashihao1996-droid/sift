import { describe, expect, it } from 'vitest'
import type { OrganizeError } from '@/types'
import { describeFailure, toOrganizeError } from './organizeError'

function error(overrides: Partial<OrganizeError> = {}): OrganizeError {
  return { kind: 'other', path: null, detail: 'boom', completed: 0, ...overrides }
}

describe('toOrganizeError', () => {
  it('keeps a structured rejection from Rust', () => {
    const rejected = {
      kind: 'diskFull',
      path: '/photos/IMG_1.ARW',
      detail: 'No space left on device',
      completed: 4,
    }

    expect(toOrganizeError(rejected)).toEqual(rejected)
  })

  it('turns a plain string or Error into an unexpected error', () => {
    expect(toOrganizeError('Task join error')).toEqual(error({ detail: 'Task join error' }))
    expect(toOrganizeError(new Error('oops')).detail).toBe('oops')
  })

  it('does not trust an unknown kind', () => {
    expect(toOrganizeError({ kind: 'nuclear', detail: 'x' }).kind).toBe('other')
  })
})

describe('describeFailure', () => {
  const archive = (step: 'delete' | 'move', deletedGroups = 0) =>
    ({ operation: 'archive', step, deletedGroups }) as const

  it('explains the category in Chinese and names the file it got stuck on', () => {
    const report = describeFailure(
      archive('move'),
      error({ kind: 'readOnly', path: 'D:\\DCIM\\IMG_1.ARW', detail: 'Read-only file system' }),
    )

    expect(report.title).toBe('归档没有完成')
    expect(report.reason).toBe('这个位置是只读的')
    expect(report.advice).toContain('写保护')
    expect(report.stuckFile).toBe('IMG_1.ARW')
    expect(report.detail).toBe('Read-only file system')
  })

  it('lists what was already deleted and archived, and sends the user back home', () => {
    const report = describeFailure(archive('move', 3), error({ kind: 'diskFull', completed: 12 }))

    expect(report.done).toEqual(['已把 3 组照片移到回收站', '已归档 12 张照片'])
    expect(report.canRetry).toBe(false)
    expect(report.advice).toContain('重新打开文件夹')
  })

  it('says archiving had not started when a delete failed', () => {
    const report = describeFailure(archive('delete', 2), error({ kind: 'automationDenied' }))

    expect(report.done).toEqual(['已把 2 组照片移到回收站', '还没有开始归档'])
    expect(report.advice).toContain('自动化')
  })

  it('counts a pair that was only partly trashed as a change', () => {
    const report = describeFailure(
      archive('delete'),
      error({ kind: 'permissionDenied', completed: 1 }),
    )

    expect(report.done).toEqual(['出错的这组已有 1 个文件移到回收站', '还没有开始归档'])
    expect(report.canRetry).toBe(false)
  })

  it('offers a retry when nothing changed on disk', () => {
    const report = describeFailure(archive('delete'), error({ kind: 'permissionDenied' }))

    expect(report.done).toEqual(['还没有改动任何文件'])
    expect(report.canRetry).toBe(true)
  })

  it('reports how far an export got; the source folder is never touched', () => {
    const report = describeFailure(
      { operation: 'export', folder: '/Volumes/Backup' },
      error({ kind: 'diskFull', completed: 5 }),
    )

    expect(report.title).toBe('导出没有完成')
    expect(report.done[0]).toBe('已导出 5 张照片到 /Volumes/Backup')
    expect(report.canRetry).toBe(true)
  })
})
