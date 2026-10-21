import { describe, expect, it } from 'vitest'
import { describeScanProgress } from './scanProgress'

const progress = (done: number, total: number) => ({ scanId: 1, done, total })

describe('describeScanProgress', () => {
  it('shows a plain scanning message until previews report progress', () => {
    expect(describeScanProgress(null)).toEqual({ label: '正在扫描文件...', percent: null })
    expect(describeScanProgress(progress(0, 0)).percent).toBeNull()
  })

  it('counts extracted previews against the total', () => {
    expect(describeScanProgress(progress(0, 5000))).toEqual({
      label: '正在提取 RAW 预览 0 / 5000',
      percent: 0,
    })
    expect(describeScanProgress(progress(1234, 5000))).toEqual({
      label: '正在提取 RAW 预览 1234 / 5000',
      percent: 24,
    })
  })

  it('never shows 100% while a preview is still being extracted', () => {
    expect(describeScanProgress(progress(4999, 5000)).percent).toBe(99)
  })

  it('says what is left once every preview is extracted', () => {
    expect(describeScanProgress(progress(5000, 5000))).toEqual({
      label: '预览已提取，正在读取拍摄时间...',
      percent: 100,
    })
  })
})
