import { describe, expect, it } from 'vitest'
import type { PhotoPair } from '@/types'
import { markToast, photoName } from './feedback'

function pair(overrides: Partial<PhotoPair> = {}): PhotoPair {
  return {
    id: 'p0',
    jpgPath: '/photos/IMG_1.jpg',
    rawPath: '/photos/IMG_1.CR3',
    rawFormat: 'CR3',
    status: 'unprocessed',
    thumbnailPath: null,
    dominantColor: null,
    source: 'jpg',
    xmpPaths: [],
    ...overrides,
  }
}

describe('photoName', () => {
  it('names a normal pair after its JPG', () => {
    expect(photoName(pair())).toBe('IMG_1.jpg')
  })

  it('names a RAW-only pair after its RAW, since its JPG is a cache file', () => {
    expect(photoName(pair({ jpgPath: 'C:\\cache\\abc.jpg', source: 'rawPreview' }))).toBe(
      'IMG_1.CR3',
    )
  })
})

describe('markToast', () => {
  it.each([
    ['star', true, '★ 精选 · IMG_1.jpg', 'star'],
    ['delete', true, '标记删除 · IMG_1.jpg', 'delete'],
    ['star', false, '已取消精选 · IMG_1.jpg', 'info'],
    ['delete', false, '已取消删除 · IMG_1.jpg', 'info'],
  ] as const)('%s, marked: %s -> %s', (type, isMarked, message, toastType) => {
    expect(markToast({ type, isMarked }, 'IMG_1.jpg')).toEqual({ message, type: toastType })
  })
})
