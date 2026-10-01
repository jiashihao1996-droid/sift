import { describe, expect, it } from 'vitest'
import { getErrorMessage } from './error'

describe('getErrorMessage', () => {
  it('returns the string a Tauri command rejected with', () => {
    expect(getErrorMessage('Failed to trash RAW: denied')).toBe('Failed to trash RAW: denied')
  })

  it('returns the message of an Error instance', () => {
    expect(getErrorMessage(new Error('boom'))).toBe('boom')
  })

  it('falls back when the value carries no message', () => {
    expect(getErrorMessage(undefined, 'fallback')).toBe('fallback')
    expect(getErrorMessage('', 'fallback')).toBe('fallback')
    expect(getErrorMessage(new Error(''), 'fallback')).toBe('fallback')
    expect(getErrorMessage({ code: 1 }, 'fallback')).toBe('fallback')
  })
})
