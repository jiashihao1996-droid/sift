import { beforeEach, describe, expect, it } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useViewStore } from './viewStore'

beforeEach(() => {
  setActivePinia(createPinia())
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
