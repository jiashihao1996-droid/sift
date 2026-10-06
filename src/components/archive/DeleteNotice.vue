<script setup lang="ts">
// ============================================================
// DeleteNotice - What deleting will do in this folder. Without a trash
// (network share, FAT/exFAT card on Windows) it is permanent and must be
// confirmed; on macOS the first delete asks for Finder access. Previews
// saved for RAW-only picks that were unstarred count too: archiving drops them.
// ============================================================

import { computed } from 'vue'
import { TriangleAlert } from '@lucide/vue'
import type { NoTrashReason } from '@/types'
import { describeNoTrash } from '@/core/noTrash'
import { IS_MAC } from '@/utils/platform'

const props = defineProps<{
  noTrash: NoTrashReason | null
  deletedCount: number
  /** Unstarred picks whose saved preview JPG archiving may delete */
  previewCount: number
}>()

/** The user confirmed permanent deletion for this archive */
const confirmed = defineModel<boolean>('confirmed', { required: true })

/** What the checkbox confirms, naming only what will actually go */
const confirmText = computed(() => {
  const parts = []
  if (props.deletedCount > 0) parts.push(`这 ${props.deletedCount} 组照片`)
  if (props.previewCount > 0) parts.push(`取消精选的纯 RAW 照片的预览 JPG`)
  return `我知道${parts.join('和')}会被永久删除，无法恢复`
})
</script>

<template>
  <template v-if="deletedCount + previewCount > 0">
    <div
      v-if="noTrash"
      class="bg-sift-delete/10 border border-sift-delete/40 rounded-lg px-3 py-3 mb-5"
      role="alert"
    >
      <div class="flex items-start gap-2 mb-2">
        <TriangleAlert :size="16" class="text-sift-delete shrink-0 mt-0.5" />
        <div>
          <p class="text-sm font-medium text-white">此位置不支持回收站，删除后无法恢复</p>
          <p class="text-[11px] text-sift-muted mt-0.5">{{ describeNoTrash(noTrash) }}</p>
        </div>
      </div>
      <label class="flex items-start gap-2 text-xs text-white/90 cursor-pointer select-none">
        <input v-model="confirmed" type="checkbox" class="mt-0.5 accent-[var(--sift-delete)]" />
        {{ confirmText }}
      </label>
    </div>

    <p v-else-if="IS_MAC" class="text-[11px] text-sift-muted mb-5 leading-relaxed">
      第一次删除时，macOS 会询问是否允许 Sift 控制「访达」：Sift
      通过「访达」把照片移到废纸篓，这样可以「放回原处」。请选择「好」。
    </p>
  </template>
</template>
