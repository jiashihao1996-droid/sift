// ============================================================
// Sift - Keyboard Composable
// Global keyboard shortcuts for culling workflow
// ============================================================

import { onMounted, onUnmounted } from 'vue'
import { useSessionStore } from '@/stores/sessionStore'
import { useViewStore } from '@/stores/viewStore'
import { IS_MAC } from '@/utils/platform'
import { groupMarkToast, type GroupMarkMode } from '@/core/grouping'
import { useSurveyToggle } from './useSurveyToggle'

const KEY_ZOOM_STEP = 1.25

/**
 * `[` / `]` by physical key, so they still work while a Chinese input method
 * turns them into 【 / 】; only unshifted, as Shift gives { / }
 */
function bracketKey(e: KeyboardEvent): '[' | ']' | null {
  if (e.shiftKey || e.metaKey || e.ctrlKey || e.altKey) return null
  if (e.code === 'BracketLeft') return '['
  if (e.code === 'BracketRight') return ']'
  return null
}

export function useKeyboard() {
  const session = useSessionStore()
  const view = useViewStore()
  const { toggleSurvey } = useSurveyToggle()

  function markGroup(mode: GroupMarkMode) {
    const plan = session.markGroup(mode)
    if (!plan) return
    const toast = groupMarkToast(plan, mode)
    view.showToast(toast.message, toast.type)
  }

  function handleKeyDown(e: KeyboardEvent) {
    // Skip if in input/textarea
    const tag = (e.target as HTMLElement)?.tagName
    if (tag === 'INPUT' || tag === 'TEXTAREA') return

    // Only active in culling view
    if (view.currentView !== 'culling') return

    // Don't handle if archive dialog is open
    if (view.showArchiveDialog) return

    // Don't handle if summary card is open
    if (view.showSummaryCard) return

    if (view.showShortcutHelp) {
      if (e.key === 'Escape' || e.key === '?') {
        e.preventDefault()
        view.closeShortcutHelp()
      }
      return
    }

    if (e.key === '?') {
      e.preventDefault()
      view.openShortcutHelp()
      return
    }

    // ESC: survey > compare mode > filter gallery
    if (e.key === 'Escape') {
      if (view.surveyMode) {
        e.preventDefault()
        view.exitSurvey()
        return
      }
      if (view.compareMode) {
        e.preventDefault()
        view.exitCompare()
        return
      }
      if (view.filterCategory) {
        e.preventDefault()
        view.closeFilterGallery()
        return
      }
    }

    // Don't handle other keys if filter gallery is open
    if (view.filterCategory) return

    const isCtrlOrCmd = IS_MAC ? e.metaKey : e.ctrlKey

    // Cmd/Ctrl + Enter: open archive dialog
    if (isCtrlOrCmd && e.key === 'Enter') {
      if (session.starredCount + session.deletedCount > 0) {
        e.preventDefault()
        view.toggleArchiveDialog()
      }
      return
    }

    switch (bracketKey(e) ?? e.key) {
      // Mark keys ignore auto-repeat: holding one must not mark a run of photos.
      // Shift decides the whole burst; Caps Lock alone also gives 'F', so check shiftKey.
      case 'f':
      case 'F':
        e.preventDefault()
        if (e.repeat) break
        if (e.shiftKey) markGroup('pick')
        else session.markStar()
        break

      case 'x':
      case 'X':
        e.preventDefault()
        if (e.repeat) break
        if (e.shiftKey) markGroup('reject')
        else session.markDelete()
        break

      case 'Delete':
        e.preventDefault()
        if (!e.repeat) session.markDelete()
        break

      // Option may be needed to type a bracket on non-US layouts; Cmd/Ctrl never
      case ']':
        if (!isCtrlOrCmd) {
          e.preventDefault()
          session.goToNextGroup()
        }
        break

      case '[':
        if (!isCtrlOrCmd) {
          e.preventDefault()
          session.goToGroupStart()
        }
        break

      case 'l':
      case 'L':
        if (!isCtrlOrCmd) {
          e.preventDefault()
          if (!e.repeat) view.toggleStayAfterMark()
        }
        break

      case ' ':
        e.preventDefault()
        session.goNext()
        break

      case 'ArrowRight':
        if (!isCtrlOrCmd) {
          e.preventDefault()
          session.goNext()
        }
        break

      case 'ArrowLeft':
      case 'a':
      case 'A':
        if (!isCtrlOrCmd) {
          e.preventDefault()
          session.goPrev()
        }
        break

      case 'z':
      case 'Z':
        if (isCtrlOrCmd) {
          e.preventDefault()
          // Shift+Cmd+Z is the platform's redo, which Sift does not offer
          if (!e.shiftKey && session.undo()) view.showToast('已撤销 ↩️', 'undo')
        } else if (!e.ctrlKey && !e.metaKey && !e.altKey) {
          e.preventDefault()
          view.toggleActualSize()
        }
        break

      case 'i':
      case 'I':
        if (!isCtrlOrCmd) {
          e.preventDefault()
          view.toggleExifPanel()
        }
        break

      case 'n':
      case 'N':
        if (!isCtrlOrCmd) {
          e.preventDefault()
          if (!e.repeat) toggleSurvey()
        }
        break

      case 'c':
      case 'C':
        if (!isCtrlOrCmd) {
          e.preventDefault()
          view.toggleCompare(session.currentIndex)
        }
        break

      case 'r':
      case 'R':
        if (!isCtrlOrCmd) {
          e.preventDefault()
          view.rotateBy(90)
        }
        break

      case '0':
        e.preventDefault()
        view.resetZoom()
        break

      case '1':
        e.preventDefault()
        if (!view.zoomToFocus()) view.zoomToActualSize()
        break

      case 'p':
      case 'P':
        if (!isCtrlOrCmd) {
          e.preventDefault()
          if (!e.repeat) view.toggleAfPoint()
        }
        break

      case '=':
      case '+':
        e.preventDefault()
        view.setZoom(view.zoomLevel * KEY_ZOOM_STEP)
        break

      case '-':
        e.preventDefault()
        view.setZoom(view.zoomLevel / KEY_ZOOM_STEP)
        break
    }
  }

  onMounted(() => {
    window.addEventListener('keydown', handleKeyDown)
  })

  onUnmounted(() => {
    window.removeEventListener('keydown', handleKeyDown)
  })

  return { isMac: IS_MAC }
}
