<script setup lang="ts">
// ============================================================
// DeleteNotice - What deleting will do in this folder. Without a trash
// (network share, FAT/exFAT card on Windows) it is permanent and must be
// confirmed; on macOS the first delete asks for Finder access.
// ============================================================

import { TriangleAlert } from 'lucide-vue-next'
import type { NoTrashReason } from '@/types'
import { describeNoTrash } from '@/core/noTrash'
import { IS_MAC } from '@/utils/platform'

defineProps<{ noTrash: NoTrashReason | null; deletedCount: number }>()

/** The user confirmed permanent deletion for this archive */
const confirmed = defineModel<boolean>('confirmed', { required: true })
</script>

<template>
  <template v-if="deletedCount > 0">
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
        我知道这 {{ deletedCount }} 组照片会被永久删除，无法恢复
      </label>
    </div>

    <p v-else-if="IS_MAC" class="text-[11px] text-sift-muted mb-5 leading-relaxed">
      第一次删除时，macOS 会询问是否允许 Sift 控制「访达」：Sift
      通过「访达」把照片移到废纸篓，这样可以「放回原处」。请选择「好」。
    </p>
  </template>
</template>
