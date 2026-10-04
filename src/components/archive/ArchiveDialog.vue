<script setup lang="ts">
// ============================================================
// ArchiveDialog - Modal for archiving, deleting only, and exporting photos
// ============================================================

import { ref, computed, watch } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { CheckCircle, Download, Archive, Trash2 } from 'lucide-vue-next'
import { useSessionStore } from '@/stores/sessionStore'
import { useViewStore } from '@/stores/viewStore'
import {
  archivePhotos,
  exportPicks,
  deletePairs,
  folderNoTrashReason,
  onArchiveProgress,
  onDeleteProgress,
  onExportProgress,
} from '@/services/tauriCommands'
import { PhotoStatus, type DeletePairInput, type NoTrashReason } from '@/types'
import { runArchive, runDeleteOnly } from '@/core/archiveFlow'
import { needsPermanentConfirm } from '@/core/noTrash'
import {
  describeDeleted,
  describeFailure,
  toOrganizeError,
  type FailureReport,
} from '@/core/organizeError'
import { useOrganizeRun } from '@/composables/useOrganizeRun'
import RollingNumber from '@/components/status/RollingNumber.vue'
import ArchiveFailure from './ArchiveFailure.vue'
import DeleteNotice from './DeleteNotice.vue'
import OrganizeNotice from './OrganizeNotice.vue'
import OrganizeProgress from './OrganizeProgress.vue'

const session = useSessionStore()
const view = useViewStore()
const { currentFile, isCloseBlocked, percent, run: runOrganize } = useOrganizeRun()

/** What is running, for the progress title */
const activity = ref<'archive' | 'delete' | 'export' | null>(null)
const ACTIVITY_TITLES = { archive: '归档中...', delete: '删除中...', export: '导出中...' }
const isComplete = ref(false)
const resultMessage = ref('')
/** Set when archiving, deleting or exporting stopped partway */
const failure = ref<FailureReport | null>(null)
/** The last delete-only run or export, shown on the options page */
const notice = ref<{ message: string; folder: string | null } | null>(null)
/** Why deleting in this folder would be permanent; null when it has a trash */
const noTrash = ref<NoTrashReason | null>(null)
/** Asked anew for every run that deletes, never remembered */
const isPermanentConfirmed = ref(false)

const starredPairs = computed(() => session.pairs.filter((p) => p.status === PhotoStatus.Starred))

const deletedPairs = computed(() => session.pairs.filter((p) => p.status === PhotoStatus.Deleted))

const mustConfirmPermanent = computed(() =>
  needsPermanentConfirm(noTrash.value, deletedPairs.value.length),
)

// Check the folder each time the dialog opens; a failed check means "has a
// trash", and Rust re-checks every file before deleting anything for good
watch(
  () => view.showArchiveDialog,
  async (isOpen) => {
    if (!isOpen) return
    isPermanentConfirmed.value = false
    noTrash.value = await folderNoTrashReason(session.folderPath).catch((e) => {
      console.warn('[Sift] Trash check failed:', e)
      return null
    })
  },
  { immediate: true },
)

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
    to: noTrash.value ? '→ 永久删除' : '→ 回收站',
  },
  {
    label: '未标记',
    count: unprocessedCount.value,
    border: 'border-sift-accent',
    to: 'JPG 不动，RAW → RAW/',
  },
])

/**
 * Whether this run deletes for good, or null when that still needs the
 * user's confirmation. The confirmation covers this run only: a retry asks
 * again.
 */
function takePermanentConfirmation(): boolean | null {
  if (mustConfirmPermanent.value && !isPermanentConfirmed.value) return null
  isPermanentConfirmed.value = false
  return mustConfirmPermanent.value
}

function deleteDeps(isPermanent: boolean) {
  return {
    deletePairs: (pairs: DeletePairInput[]) => deletePairs(pairs, isPermanent),
    dropPairs: session.dropPairs,
  }
}

async function handleArchive() {
  const isPermanent = takePermanentConfirmation()
  if (isPermanent === null) return
  // Captured now: the progress to forget is this folder's, whatever happens next
  const folder = session.folderPath
  activity.value = 'archive'
  notice.value = null
  try {
    const outcome = await runOrganize([onDeleteProgress, onArchiveProgress], () =>
      runArchive(
        folder,
        session.pairs,
        { ...deleteDeps(isPermanent), archivePhotos },
        { isPermanent },
      ),
    )
    if (!outcome.ok) {
      failure.value = outcome.report
      return
    }
    resultMessage.value = `已删除 ${outcome.deletedGroups} 组，归档 ${outcome.movedCount} 个文件`
    // Files have moved into 精选/ and RAW/: the session and its saved marks no longer apply
    session.markArchived(folder)
    isComplete.value = true
  } catch (e) {
    // runArchive reports its own failures; this is listening for progress failing
    const stage = { operation: 'archive', step: 'move', deletedGroups: 0 } as const
    failure.value = describeFailure(stage, toOrganizeError(e))
  } finally {
    activity.value = null
  }
}

/** Delete the marked photos and nothing else; culling can go on afterwards */
async function handleDeleteOnly() {
  const isPermanent = takePermanentConfirmation()
  if (isPermanent === null) return
  const marked = deletedPairs.value.length
  activity.value = 'delete'
  notice.value = null
  try {
    const outcome = await runOrganize([onDeleteProgress], () =>
      runDeleteOnly(session.pairs, deleteDeps(isPermanent), { isPermanent }),
    )
    if (!outcome.ok) {
      failure.value = outcome.report
      return
    }
    notice.value = { message: describeDeleted(outcome.deletedGroups, isPermanent), folder: null }
  } catch (e) {
    const stage = {
      operation: 'delete',
      step: 'delete',
      deletedGroups: 0,
      failedGroups: marked,
      isPermanent,
    } as const
    failure.value = describeFailure(stage, toOrganizeError(e))
  } finally {
    activity.value = null
  }
}

async function handleExport() {
  const selected = await open({
    directory: true,
    multiple: false,
    title: '选择导出目标',
  })

  if (!selected || typeof selected !== 'string') return

  activity.value = 'export'
  notice.value = null
  try {
    const pairsData = starredPairs.value.map((p) => ({
      jpgPath: p.jpgPath,
      rawPath: p.rawPath,
      source: p.source,
      xmpPaths: p.xmpPaths,
    }))

    const result = await runOrganize([onExportProgress], () => exportPicks(pairsData, selected))
    // Back to the options: exporting picks is often followed by archiving
    notice.value = {
      message: `已导出 ${result.exportedCount} 个照片文件`,
      folder: result.exportFolder,
    }
  } catch (e) {
    failure.value = describeFailure({ operation: 'export', folder: selected }, toOrganizeError(e))
  } finally {
    activity.value = null
  }
}

function close() {
  if (view.isOrganizing) return
  // An archived session no longer matches the folder: the only way on is home
  if (session.isArchived) return
  // After a partial archive the session no longer matches the folder: the
  // only way on is home, so the backdrop does not dismiss that failure
  if (failure.value && !failure.value.canRetry) return
  failure.value = null
  notice.value = null
  view.toggleArchiveDialog()
}

function done() {
  failure.value = null
  notice.value = null
  session.resetSession()
  view.setView('welcome')
  view.toggleArchiveDialog()
}
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

        <OrganizeProgress
          v-else-if="activity"
          :title="ACTIVITY_TITLES[activity]"
          :percent="percent"
          :current-file="currentFile"
          :is-close-blocked="isCloseBlocked"
        />

        <!-- Default: Archive Options -->
        <div v-else>
          <h3 class="text-lg font-bold text-white text-center mb-1">归档与导出</h3>
          <p class="text-xs text-sift-muted text-center mb-5">整理你的照片</p>

          <OrganizeNotice v-if="notice" :message="notice.message" :folder="notice.folder" />

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

          <DeleteNotice
            v-model:confirmed="isPermanentConfirmed"
            :no-trash="noTrash"
            :deleted-count="deletedPairs.length"
          />

          <!-- Actions -->
          <div class="space-y-3">
            <button
              class="w-full h-12 rounded-xl bg-gradient-to-r from-sift-accent to-blue-500 text-white font-medium text-sm flex items-center justify-center gap-2 btn-spring btn-glow shadow-lg shadow-sift-accent/20 disabled:opacity-40 disabled:pointer-events-none"
              :disabled="mustConfirmPermanent && !isPermanentConfirmed"
              @click="handleArchive"
            >
              <Archive :size="16" />
              归档
            </button>

            <button
              v-if="deletedPairs.length > 0"
              class="w-full h-11 rounded-xl border border-sift-delete/40 text-sift-text text-sm flex items-center justify-center gap-2 hover:bg-sift-delete/10 transition-colors btn-spring disabled:opacity-40 disabled:pointer-events-none"
              :disabled="mustConfirmPermanent && !isPermanentConfirmed"
              title="只处理标记删除的照片，其他文件都不移动，删完可以接着筛"
              @click="handleDeleteOnly"
            >
              <Trash2 :size="16" />
              只删除，不整理（{{ deletedPairs.length }} 组）
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
