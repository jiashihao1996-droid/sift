import { describe, expect, it } from 'vitest'
import { PhotoStatus, type PhotoPair } from '@/types'
import { describeNoTrash, mayDropSavedPreview, needsPermanentConfirm } from './noTrash'

describe('needsPermanentConfirm', () => {
  it('asks only when photos would be deleted somewhere without a trash', () => {
    expect(needsPermanentConfirm('removableFat', 3)).toBe(true)
    expect(needsPermanentConfirm('networkVolume', 1)).toBe(true)
    expect(needsPermanentConfirm('networkVolume', 0)).toBe(false)
    expect(needsPermanentConfirm(null, 3)).toBe(false)
  })
})

describe('describeNoTrash', () => {
  it('names the kind of place in plain words', () => {
    expect(describeNoTrash('networkVolume')).toContain('网络共享')
    expect(describeNoTrash('removableFat')).toContain('存储卡')
  })
})

describe('mayDropSavedPreview', () => {
  const pick: PhotoPair = {
    id: 'p1',
    jpgPath: '/photos/精选/DSC_1.jpg',
    rawPath: '/photos/精选/DSC_1.ARW',
    rawFormat: 'ARW',
    status: PhotoStatus.Unprocessed,
    thumbnailPath: null,
    dominantColor: null,
    source: 'jpg',
    xmpPaths: [],
    captureTime: null,
    isArchivedPick: true,
  }

  it('covers an unstarred archived pick whose JPG is named after its RAW', () => {
    expect(mayDropSavedPreview(pick)).toBe(true)
    expect(mayDropSavedPreview({ ...pick, jpgPath: 'C:\\photos\\精选\\DSC_1.jpg' })).toBe(true)
  })

  it('leaves out picks still starred or marked for deletion', () => {
    expect(mayDropSavedPreview({ ...pick, status: PhotoStatus.Starred })).toBe(false)
    expect(mayDropSavedPreview({ ...pick, status: PhotoStatus.Deleted })).toBe(false)
  })

  it('leaves out photos outside 精选/, JPG-only picks and differently named JPGs', () => {
    expect(mayDropSavedPreview({ ...pick, isArchivedPick: false })).toBe(false)
    expect(mayDropSavedPreview({ ...pick, rawPath: null })).toBe(false)
    expect(mayDropSavedPreview({ ...pick, jpgPath: '/photos/精选/DSC_1_edit.jpg' })).toBe(false)
  })
})
