// ============================================================
// Sift - Survey Toggle
// N and the viewer's grid button: a photo on its own has no burst to survey
// ============================================================

import { useSessionStore } from '@/stores/sessionStore'
import { useViewStore } from '@/stores/viewStore'

export function useSurveyToggle() {
  const session = useSessionStore()
  const view = useViewStore()

  function toggleSurvey() {
    if (!view.surveyMode && !session.currentGroupPosition) {
      view.showToast('这张不在连拍组里', 'info')
      return
    }
    view.toggleSurvey()
  }

  return { toggleSurvey }
}
