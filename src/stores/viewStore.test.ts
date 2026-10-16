import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useViewStore } from './viewStore'
import { setOrganizing } from '@/services/tauriCommands'

vi.mock('@/services/tauriCommands', () => ({ setOrganizing: vi.fn(async () => {}) }))

const STORAGE_KEY = 'sift.stayAfterMark'

function stubStorage(initial: Record<string, string> = {}) {
  const data = new Map(Object.entries(initial))
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => data.get(key) ?? null,
    setItem: (key: string, value: string) => data.set(key, value),
  })
  return data
}

beforeEach(() => {
  stubStorage()
  setActivePinia(createPinia())
})

afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
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

describe('stayAfterMark', () => {
  it('is off by default, so marking keeps moving on', () => {
    expect(useViewStore().stayAfterMark).toBe(false)
  })

  it('restores the saved preference', () => {
    stubStorage({ [STORAGE_KEY]: 'true' })
    expect(useViewStore().stayAfterMark).toBe(true)
  })

  it('saves each toggle and announces the new state', () => {
    const data = stubStorage()
    const view = useViewStore()

    view.toggleStayAfterMark()
    expect([view.stayAfterMark, data.get(STORAGE_KEY)]).toEqual([true, 'true'])
    expect(view.toastMessage).toContain('停在当前照片')

    view.toggleStayAfterMark()
    expect([view.stayAfterMark, data.get(STORAGE_KEY)]).toEqual([false, 'false'])
    expect(view.toastMessage).toContain('自动前进')
  })

  it('falls back to the default and still toggles when storage is unavailable', () => {
    vi.stubGlobal('localStorage', {
      getItem: () => {
        throw new Error('denied')
      },
      setItem: () => {
        throw new Error('denied')
      },
    })
    vi.spyOn(console, 'warn').mockImplementation(() => {})
    const view = useViewStore()

    expect(view.stayAfterMark).toBe(false)
    view.toggleStayAfterMark()
    expect(view.stayAfterMark).toBe(true)
  })
})

describe('showAfPoint', () => {
  const AF_KEY = 'sift.showAfPoint'

  it('is off by default and restores the saved preference', () => {
    expect(useViewStore().showAfPoint).toBe(false)
    setActivePinia(createPinia())
    stubStorage({ [AF_KEY]: 'true' })
    expect(useViewStore().showAfPoint).toBe(true)
  })

  it('saves each toggle and announces the new state', () => {
    const data = stubStorage()
    const view = useViewStore()

    view.toggleAfPoint()
    expect([view.showAfPoint, data.get(AF_KEY)]).toEqual([true, 'true'])
    expect(view.toastMessage).toContain('显示对焦点')

    view.toggleAfPoint()
    expect([view.showAfPoint, data.get(AF_KEY)]).toEqual([false, 'false'])
    expect(view.toastMessage).toBe('隐藏对焦点')
  })
})

describe('zoomToFocus', () => {
  function viewWithFocus() {
    setActivePinia(createPinia())
    const view = useViewStore()
    view.toggleAfPoint()
    view.actualZoom = 3
    view.afFocusOffset = { x: 120, y: -40 }
    return view
  }

  it('zooms to 100% with the focus at the centre', () => {
    const view = viewWithFocus()

    expect(view.zoomToFocus()).toBe(true)
    expect([view.zoomLevel, view.zoomOffsetX, view.zoomOffsetY]).toEqual([3, 120, -40])
  })

  it('does nothing without a focus, before the photo is measured or with the overlay off', () => {
    const noFocus = viewWithFocus()
    noFocus.afFocusOffset = null
    expect(noFocus.zoomToFocus()).toBe(false)

    const unmeasured = viewWithFocus()
    unmeasured.actualZoom = null
    expect(unmeasured.zoomToFocus()).toBe(false)

    const off = viewWithFocus()
    off.toggleAfPoint()
    expect(off.zoomToFocus()).toBe(false)
    expect(off.zoomLevel).toBe(1)
  })
})

describe('sortOrder', () => {
  const SORT_KEY = 'sift.sortOrder'

  it('sorts by capture time by default', () => {
    expect(useViewStore().sortOrder).toBe('captureTime')
  })

  it('restores a saved order and ignores an unknown one', () => {
    stubStorage({ [SORT_KEY]: 'fileName' })
    expect(useViewStore().sortOrder).toBe('fileName')

    setActivePinia(createPinia())
    stubStorage({ [SORT_KEY]: 'shuffle' })
    expect(useViewStore().sortOrder).toBe('captureTime')
  })

  it('saves the chosen order', () => {
    const data = stubStorage()
    const view = useViewStore()

    view.setSortOrder('fileName')

    expect([view.sortOrder, data.get(SORT_KEY)]).toEqual(['fileName', 'fileName'])
  })
})

describe('burstGapSeconds', () => {
  const GAP_KEY = 'sift.burstGapSeconds'

  it('groups photos taken within 2 seconds by default', () => {
    expect(useViewStore().burstGapSeconds).toBe(2)
  })

  it('restores a saved gap and ignores one that is not offered', () => {
    stubStorage({ [GAP_KEY]: '10' })
    expect(useViewStore().burstGapSeconds).toBe(10)

    setActivePinia(createPinia())
    stubStorage({ [GAP_KEY]: '7' })
    expect(useViewStore().burstGapSeconds).toBe(2)
  })

  it('saves the chosen gap', () => {
    const data = stubStorage()
    const view = useViewStore()

    view.setBurstGapSeconds(0)

    expect([view.burstGapSeconds, data.get(GAP_KEY)]).toEqual([0, '0'])
  })
})

describe('survey and compare', () => {
  it('leaves compare when the survey opens, and the other way round', () => {
    const view = useViewStore()
    view.toggleCompare(3)

    view.toggleSurvey()
    expect([view.surveyMode, view.compareMode, view.compareIndex]).toEqual([true, false, null])

    view.toggleCompare(2)
    expect([view.surveyMode, view.compareMode, view.compareIndex]).toEqual([false, true, 2])
  })

  it('starts the survey at fit and forgets where its window was', () => {
    const view = useViewStore()
    view.setZoom(3)
    view.surveyStart = 7

    view.toggleSurvey()

    expect([view.zoomLevel, view.surveyStart]).toEqual([1, null])
  })
})

describe('organizing', () => {
  it('tells Rust while organizing so closing the window is refused', () => {
    const view = useViewStore()

    view.setOrganizing(true)
    expect(view.isOrganizing).toBe(true)
    expect(setOrganizing).toHaveBeenLastCalledWith(true)

    view.setOrganizing(false)
    expect(view.isOrganizing).toBe(false)
    expect(setOrganizing).toHaveBeenLastCalledWith(false)
  })
})
