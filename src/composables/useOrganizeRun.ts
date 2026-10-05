// ============================================================
// Sift - Organize Run
// The state of one archive, delete or export while it runs: its progress,
// and the lock that keeps the dialog, the back button, the window and the
// app from closing it off between two files.
// ============================================================

import { computed, onMounted, onUnmounted, ref } from 'vue'
import type { UnlistenFn } from '@tauri-apps/api/event'
import type { ArchiveProgress } from '@/types'
import { onCloseBlocked } from '@/services/tauriCommands'
import { useViewStore } from '@/stores/viewStore'

/** Subscribes to one kind of progress event, e.g. `onDeleteProgress` */
type ProgressSource = (callback: (progress: ArchiveProgress) => void) => Promise<UnlistenFn>

export function useOrganizeRun() {
  const view = useViewStore()
  const current = ref(0)
  const total = ref(0)
  const currentFile = ref('')
  /** The user tried to close or quit Sift during this run */
  const isCloseBlocked = ref(false)

  let stopListeningForClose: UnlistenFn | undefined
  onMounted(async () => {
    stopListeningForClose = await onCloseBlocked(() => {
      isCloseBlocked.value = true
    })
  })
  onUnmounted(() => stopListeningForClose?.())

  function showProgress(progress: ArchiveProgress) {
    current.value = progress.current
    total.value = progress.total
    currentFile.value = progress.currentFile
  }

  /**
   * Run `work` with progress from `sources` and nothing allowed to close or
   * leave it until it settles.
   */
  async function run<T>(sources: ProgressSource[], work: () => Promise<T>): Promise<T> {
    current.value = 0
    total.value = 0
    currentFile.value = ''
    isCloseBlocked.value = false
    view.setOrganizing(true)
    let unlisten: UnlistenFn[] = []
    try {
      unlisten = await Promise.all(sources.map((listen) => listen(showProgress)))
      return await work()
    } finally {
      unlisten.forEach((stop) => stop())
      view.setOrganizing(false)
    }
  }

  const percent = computed(() =>
    total.value === 0 ? 0 : Math.round((current.value / total.value) * 100),
  )

  return { currentFile, isCloseBlocked, percent, run }
}
