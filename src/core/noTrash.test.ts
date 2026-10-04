import { describe, expect, it } from 'vitest'
import { describeNoTrash, needsPermanentConfirm } from './noTrash'

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
