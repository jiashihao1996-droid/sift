// ============================================================
// Sift - Save progress when leaving
// Progress is saved a moment after changes settle; hiding the window,
// closing it or quitting saves it at once. A close or quit waits in Rust
// until it hears back (commands/closing.rs).
// ============================================================

import { onSaveBeforeClose, progressSaved } from '@/services/tauriCommands'
import { useSessionStore } from '@/stores/sessionStore'

/** The part of `document` this needs; tests pass a stand-in */
export interface VisiblePage {
  readonly visibilityState: DocumentVisibilityState
  addEventListener(type: 'visibilitychange', listener: () => void): void
}

export function useSaveOnLeave(page: VisiblePage = document) {
  const session = useSessionStore()

  page.addEventListener('visibilitychange', () => {
    if (page.visibilityState === 'hidden') void session.flushProgress()
  })

  void onSaveBeforeClose(async () => {
    try {
      await session.flushProgress()
    } finally {
      await progressSaved().catch((e) => console.warn('[Sift] Failed to report saving:', e))
    }
  })
}
