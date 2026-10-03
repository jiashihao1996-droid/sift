import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useViewStore } from './viewStore'

beforeEach(() => {
  setActivePinia(createPinia())
})

afterEach(() => {
  vi.useRealTimers()
})

describe('setView', () => {
  it('closes the summary card so it does not reappear over the next folder', () => {
    const view = useViewStore()
    view.setView('culling')
    view.showSummaryCard = true

    view.setView('welcome')
    view.setView('culling')

    expect(view.showSummaryCard).toBe(false)
  })
})

describe('showToast', () => {
  it('hides the toast after its duration', () => {
    vi.useFakeTimers()
    const view = useViewStore()

    view.showToast('saved', 'info', 1000)
    vi.advanceTimersByTime(999)
    expect(view.toastVisible).toBe(true)

    vi.advanceTimersByTime(1)
    expect(view.toastVisible).toBe(false)
  })

  it('replaces the previous toast without the old timer cutting the new one short', () => {
    vi.useFakeTimers()
    const view = useViewStore()

    view.showToast('first', 'star', 1000)
    vi.advanceTimersByTime(800)
    view.showToast('second', 'delete', 1000)
    vi.advanceTimersByTime(500)

    expect(view.toastVisible).toBe(true)
    expect([view.toastMessage, view.toastType]).toEqual(['second', 'delete'])
  })
})
