import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { onSaveBeforeClose, progressSaved } from '@/services/tauriCommands'
import { useSessionStore } from '@/stores/sessionStore'
import { useSaveOnLeave } from './useSaveOnLeave'

vi.mock('@/services/tauriCommands', () => ({
  onSaveBeforeClose: vi.fn(async () => () => {}),
  progressSaved: vi.fn(async () => {}),
  saveProgress: vi.fn(async () => {}),
  clearProgress: vi.fn(async () => {}),
}))

class FakePage extends EventTarget {
  visibilityState: DocumentVisibilityState = 'visible'

  show(state: DocumentVisibilityState) {
    this.visibilityState = state
    this.dispatchEvent(new Event('visibilitychange'))
  }
}

/** The handler registered for "save before closing" */
function saveBeforeClose(): () => Promise<void> {
  const handler = vi.mocked(onSaveBeforeClose).mock.calls.at(-1)?.[0]
  if (!handler) throw new Error('no handler registered')
  return handler as () => Promise<void>
}

describe('useSaveOnLeave', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  it('saves progress when the window is hidden', () => {
    const session = useSessionStore()
    const flush = vi.spyOn(session, 'flushProgress')
    const page = new FakePage()
    useSaveOnLeave(page)

    page.show('hidden')

    expect(flush).toHaveBeenCalledOnce()
    page.show('visible')
    expect(flush).toHaveBeenCalledOnce()
  })

  it('lets a waiting close go ahead once progress is saved', async () => {
    const session = useSessionStore()
    const order: string[] = []
    vi.spyOn(session, 'flushProgress').mockImplementation(async () => {
      order.push('saved')
    })
    vi.mocked(progressSaved).mockImplementation(async () => {
      order.push('reported')
    })
    useSaveOnLeave(new FakePage())

    await saveBeforeClose()()

    expect(order).toEqual(['saved', 'reported'])
  })

  it('lets the close go ahead even when saving fails', async () => {
    const session = useSessionStore()
    vi.spyOn(session, 'flushProgress').mockRejectedValue(new Error('disk full'))
    useSaveOnLeave(new FakePage())

    await expect(saveBeforeClose()()).rejects.toThrow('disk full')

    expect(progressSaved).toHaveBeenCalledOnce()
  })
})
