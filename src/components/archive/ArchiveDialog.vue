<script setup lang="ts">
// ============================================================
// ArchiveDialog - Modal for archiving and exporting photos
// ============================================================

import { ref, computed } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { CheckCircle, Download, Archive } from 'lucide-vue-next'
import { useSessionStore } from '@/stores/sessionStore'
import { useViewStore } from '@/stores/viewStore'
import {
  archivePhotos,
  exportPicks,
  deletePairs,
  onArchiveProgress,
  onDeleteProgress,
} from '@/services/tauriCommands'
import { PhotoStatus, type ArchiveProgress } from '@/types'
import { runArchive } from '@/core/archiveFlow'
import RollingNumber from '@/components/status/RollingNumber.vue'
import ArchiveFailure from './ArchiveFailure.vue'
import { describeFailure, toOrganizeError, type FailureReport } from '@/core/organizeError'

const session = useSessionStore()
const view = useViewStore()

const isArchiving = ref(false)
const isExporting = ref(false)
const isComplete = ref(false)
const archiveProgress = ref(0)
const archiveTotal = ref(0)
const currentFile = ref('')
const resultMessage = ref('')
/** Set when archiving or exporting stopped partway */
const failure = ref<FailureReport | null>(null)

const starredPairs = computed(() => session.pairs.filter((p) => p.status === PhotoStatus.Starred))

const deletedPairs = computed(() => session.pairs.filter((p) => p.status === PhotoStatus.Deleted))

/** Starred RAW-only photos get their preview saved as a JPG next to the RAW */
const starredRawOnlyCount = computed(
  () => starredPairs.value.filter((p) => p.source === 'rawPreview').length,
)

const unprocessedCount = computed(
  () => session.pairs.filter((p) => p.status === PhotoStatus.Unprocessed).length,
)

/** Where each kind of photo ends up, shown before archiving */
const destinations = computed(() => [
  { label: '精选', count: starredPairs.value.length, border: 'border-sift-star', to: '→ 精选/' },
  {
    label: '删除',
    count: deletedPairs.value.length,
    border: 'border-sift-delete',
    to: '→ 回收站',
  },
  {
    label: '未标记',
    count: unprocessedCount.value,
    border: 'border-sift-accent',
    to: 'JPG 不动，RAW → RAW/',
  },
])

function showProgress(progress: ArchiveProgress) {
  archiveProgress.value = progress.current
  archiveTotal.value = progress.total
  currentFile.value = progress.currentFile
}

async function handleArchive() {
  isArchiving.value = true
  archiveProgress.value = 0
  archiveTotal.value = 0
  currentFile.value = ''
  let unlisten: UnlistenFn[] = []

  try {
    unlisten = await Promise.all([onDeleteProgress(showProgress), onArchiveProgress(showProgress)])
    const outcome = await runArchive(session.folderPath, session.pairs, {
      deletePairs,
      archivePhotos,
      dropPairs: session.dropPairs,
    })
    if (!outcome.ok) {
      failure.value = outcome.report
      return
    }
    resultMessage.value = `已删除 ${outcome.deletedGroups} 组，归档 ${outcome.movedCount} 个文件`
    // Files have moved into 精选/ and RAW/; the saved marks no longer apply
    session.forgetProgress()
    isComplete.value = true
  } catch (e) {
    // runArchive reports its own failures; this is listening for progress failing
    const stage = { operation: 'archive', step: 'move', deletedGroups: 0 } as const
    failure.value = describeFailure(stage, toOrganizeError(e))
  } finally {
    unlisten.forEach((stop) => stop())
    isArchiving.value = false
  }
}

async function handleExport() {
  const selected = await open({
    directory: true,
    multiple: false,
    title: '选择导出目标',
  })

  if (!selected || typeof selected !== 'string') return

  isExporting.value = true
  try {
    const pairsData = starredPairs.value.map((p) => ({
      jpgPath: p.jpgPath,
      rawPath: p.rawPath,
      source: p.source,
      xmpPaths: p.xmpPaths,
    }))

    const result = await exportPicks(pairsData, selected)
    resultMessage.value = `已导出 ${result.exportedCount} 个标记文件至 ${result.exportFolder}`
    isComplete.value = true
  } catch (e) {
    failure.value = describeFailure({ operation: 'export', folder: selected }, toOrganizeError(e))
  } finally {
    isExporting.value = false
  }
}

function close() {
  // After a partial archive the session no longer matches the folder: the
  // only way on is home, so the backdrop does not dismiss that failure
  if (failure.value && !failure.value.canRetry) return
  failure.value = null
  view.toggleArchiveDialog()
}

function done() {
  failure.value = null
  session.resetSession()
  view.setView('welcome')
  view.toggleArchiveDialog()
}

const progressPercent = computed(() => {
  if (archiveTotal.value === 0) return 0
  return Math.round((archiveProgress.value / archiveTotal.value) * 100)
})
</script>

<template>
  <!-- Backdrop and dialog are sibling Transitions so each one plays its own enter/leave -->
  <Transition name="backdrop">
    <div
      v-if="view.showArchiveDialog"
      class="fixed inset-0 z-50 bg-black/60 backdrop-blur-sm"
      @click="close"
    />
  </Transition>

  <Transition name="modal">
    <div
      v-if="view.showArchiveDialog"
      class="fixed inset-0 z-50 flex items-center justify-center pointer-events-none"
    >
      <div
        class="relative max-w-lg w-full mx-4 p-6 rounded-2xl bg-sift-surface border border-white/5 shadow-2xl pointer-events-auto"
      >
        <!-- Complete State -->
        <div v-if="isComplete" class="text-center py-4">
          <div
            class="mx-auto w-16 h-16 rounded-full bg-sift-success/20 flex items-center justify-center mb-4"
          >
            <CheckCircle :size="32" class="text-sift-success" />
          </div>
          <h3 class="text-lg font-bold text-white mb-2">完成！</h3>
          <p class="text-sm text-sift-muted mb-6">{{ resultMessage }}</p>
          <button
            class="w-full h-11 rounded-xl bg-sift-card text-white text-sm hover:bg-sift-border transition-colors btn-spring"
            @click="done"
          >
            返回首页
          </button>
        </div>

        <ArchiveFailure
          v-else-if="failure"
          :report="failure"
          @retry="failure = null"
          @home="done"
        />

        <!-- Archiving Progress -->
        <div v-else-if="isArchiving || isExporting" class="py-4">
          <h3 class="text-lg font-bold text-white text-center mb-4">
            {{ isArchiving ? '归档中...' : '导出中...' }}
          </h3>
          <!-- Progress Bar -->
          <div class="h-2 bg-sift-card rounded-full overflow-hidden mb-3">
            <div
              class="h-full rounded-full bg-gradient-to-r from-sift-accent to-blue-400 transition-all duration-300 relative overflow-hidden"
              :style="{ width: `${progressPercent}%` }"
            >
              <div class="absolute inset-0 shimmer-bar" />
            </div>
          </div>
          <p class="text-xs text-sift-muted text-center truncate">
            {{ currentFile }}
          </p>
        </div>

        <!-- Default: Archive Options -->
        <div v-else>
          <h3 class="text-lg font-bold text-white text-center mb-1">归档与导出</h3>
          <p class="text-xs text-sift-muted text-center mb-5">整理你的照片</p>

          <!-- Where each kind of photo goes -->
          <div class="grid grid-cols-3 gap-3 mb-5">
            <div
              v-for="dest in destinations"
              :key="dest.label"
              class="bg-sift-card/60 rounded-xl p-3 border-l-2"
              :class="dest.border"
            >
              <p class="text-[11px] text-sift-muted">{{ dest.label }}</p>
              <p class="text-lg font-bold text-white">
                <RollingNumber :value="dest.count" />
              </p>
              <p class="text-[10px] text-sift-muted mt-0.5">{{ dest.to }}</p>
            </div>
          </div>

          <!-- RAW-only picks -->
          <div
            v-if="starredRawOnlyCount > 0"
            class="bg-amber-500/10 border border-amber-500/20 rounded-lg px-3 py-2 mb-5 flex items-start gap-2"
          >
            <span class="text-amber-400 text-xs mt-0.5">📷</span>
            <p class="text-xs text-amber-200/80">
              {{ starredRawOnlyCount }} 张精选的纯 RAW 照片会在 精选/ 里生成同名 JPG 预览
            </p>
          </div>

          <!-- Archive path -->
          <div class="bg-sift-card/40 rounded-lg px-3 py-2 mb-5">
            <p class="text-[11px] text-sift-muted mb-1">整理位置</p>
            <p class="text-xs text-white truncate">{{ session.folderPath }}/</p>
            <p class="text-[10px] text-sift-muted mt-0.5">重名时整组加 _1、_2 后缀，不会覆盖文件</p>
          </div>

          <!-- Actions -->
          <div class="space-y-3">
            <button
              class="w-full h-12 rounded-xl bg-gradient-to-r from-sift-accent to-blue-500 text-white font-medium text-sm flex items-center justify-center gap-2 btn-spring btn-glow shadow-lg shadow-sift-accent/20"
              @click="handleArchive"
            >
              <Archive :size="16" />
              归档
            </button>

            <button
              v-if="starredPairs.length > 0"
              class="w-full h-11 rounded-xl border border-sift-border text-sift-text text-sm flex items-center justify-center gap-2 hover:bg-sift-card/50 transition-colors btn-spring"
              @click="handleExport"
            >
              <Download :size="16" />
              导出 {{ starredPairs.length }} 张精选照片
            </button>

            <button
              class="w-full h-10 text-sift-muted text-xs hover:text-white transition-colors"
              @click="close"
            >
              取消
            </button>
          </div>
        </div>
      </div>
    </div>
  </Transition>
</template>
