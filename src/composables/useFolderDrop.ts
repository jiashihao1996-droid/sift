// ============================================================
// Sift - Folder drag-and-drop (Tauri native drop, not HTML5 DnD)
// Active only while the calling component is mounted
// ============================================================

import { ref, onMounted, onUnmounted } from 'vue'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { resolveDropTarget } from '@/utils/folderDrop'

export function useFolderDrop(onDrop: (folder: string) => void) {
  const isDragOver = ref(false)
  let unlisten: UnlistenFn | null = null
  let isUnmounted = false

  onMounted(async () => {
    try {
      const fn = await getCurrentWebview().onDragDropEvent(({ payload }) => {
        if (payload.type === 'enter' || payload.type === 'over') {
          isDragOver.value = true
          return
        }
        isDragOver.value = false
        if (payload.type !== 'drop') return
        const folder = resolveDropTarget(payload.paths)
        if (folder) onDrop(folder)
      })
      // Unmounted while the listener was registering
      if (isUnmounted) fn()
      else unlisten = fn
    } catch (e) {
      console.error('[Sift] Failed to register drag-and-drop listener', e)
    }
  })

  onUnmounted(() => {
    isUnmounted = true
    unlisten?.()
    unlisten = null
  })

  return { isDragOver }
}
