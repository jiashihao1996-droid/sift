<script setup lang="ts">
// ============================================================
// ToastNotification - Spring animated toast with undo support
// Also confirms every mark key press, since marking moves on to the next photo
// ============================================================

import { computed, watch } from 'vue'
import { Undo2 } from '@lucide/vue'
import { useViewStore } from '@/stores/viewStore'
import { useSessionStore } from '@/stores/sessionStore'
import { markToast, photoName } from '@/core/feedback'

/** Short enough not to linger while culling at a photo a second */
const MARK_TOAST_MS = 1200

const view = useViewStore()
const session = useSessionStore()

const bgColor = computed(() => {
  switch (view.toastType) {
    case 'star':
      return 'bg-sift-star/20 border-sift-star/30'
    case 'delete':
      return 'bg-sift-delete/20 border-sift-delete/30'
    case 'undo':
      return 'bg-sift-accent/20 border-sift-accent/30'
    default:
      return 'bg-sift-card/80 border-sift-border'
  }
})

watch(
  () => session.lastMark,
  (change) => {
    const pair = change && session.pairs[change.index]
    if (!change || !pair) return
    const toast = markToast(change, photoName(pair))
    view.showToast(toast.message, toast.type, MARK_TOAST_MS)
  },
)

function handleUndo() {
  if (session.undo()) view.showToast('已撤销 ↩️', 'undo')
}
</script>

<template>
  <Transition name="toast">
    <div
      v-if="view.toastVisible"
      class="fixed left-1/2 -translate-x-1/2 z-50 px-4 py-2.5 rounded-xl border backdrop-blur-lg flex items-center gap-3 shadow-lg"
      :class="[
        bgColor,
        // Above the settings strips when I shows them in compare / survey
        view.showsShotStrips
          ? 'bottom-[calc(var(--bottom-chrome-h)+var(--shot-strip-clearance)+1rem)]'
          : 'bottom-[calc(var(--bottom-chrome-h)+1rem)]',
      ]"
    >
      <span class="text-sm text-white/90">{{ view.toastMessage }}</span>
      <button
        v-if="view.toastType !== 'undo' && session.undoStack.length > 0"
        class="text-xs text-sift-accent hover:text-sift-accent/80 flex items-center gap-1 btn-spring"
        @click="handleUndo"
      >
        <Undo2 :size="12" />
        撤销
      </button>
    </div>
  </Transition>
</template>
