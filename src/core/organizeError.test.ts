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
  const moving = (deletedGroups = 0) =>
    ({ operation: 'archive', step: 'move', deletedGroups }) as const
  const deleting = (deletedGroups: number, failedGroups: number) =>
    ({ operation: 'archive', step: 'delete', deletedGroups, failedGroups }) as const

  it('explains the category in Chinese and names the file it got stuck on', () => {
    const report = describeFailure(
      moving(),
      error({ kind: 'readOnly', path: 'D:\\DCIM\\IMG_1.ARW', detail: 'Read-only file system' }),
    )

    expect(report.title).toBe('归档没有完成')
    expect(report.reason).toBe('这个位置是只读的')
    expect(report.advice).toContain('写保护')
    expect(report.stuckFile).toBe('IMG_1.ARW')
    expect(report.detail).toBe('Read-only file system')
  })

  it('lists what was already deleted and archived, and sends the user back home', () => {
    const report = describeFailure(moving(3), error({ kind: 'diskFull', completed: 12 }))

    expect(report.done).toEqual(['已把 3 组照片移到回收站', '已归档 12 张照片'])
    expect(report.canRetry).toBe(false)
    expect(report.advice).toContain('重新打开文件夹')
  })

  it('offers a retry when deleting failed: the session already dropped what was trashed', () => {
    const report = describeFailure(deleting(2, 1), error({ kind: 'automationDenied' }))

    expect(report.done).toEqual([
      '已把 2 组照片移到回收站',
      '1 组没有删完，仍标记为删除',
      '还没有开始归档',
    ])
    expect(report.canRetry).toBe(true)
    expect(report.advice).toContain('自动化')
  })

  it('offers a retry when no photo had moved yet', () => {
    const report = describeFailure(moving(), error({ kind: 'permissionDenied' }))

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

  it('says a delete-only run left everything else alone, and how the photos were deleted', () => {
    const report = describeFailure(
      { operation: 'delete', step: 'delete', deletedGroups: 4, failedGroups: 1, isPermanent: true },
      error({ kind: 'permissionDenied' }),
    )

    expect(report.title).toBe('删除没有完成')
    expect(report.done).toEqual([
      '已永久删除 4 组照片',
      '1 组没有删完，仍标记为删除',
      '其他照片没有任何改动',
    ])
    expect(report.canRetry).toBe(true)
  })
})
