<script setup lang="ts">
// ============================================================
// OrganizeProgress - Progress of a running archive, delete or export, and
// why Sift cannot be closed until it is done
// ============================================================

defineProps<{
  title: string
  percent: number
  currentFile: string
  /** The user tried to close or quit Sift during this run */
  isCloseBlocked: boolean
}>()
</script>

<template>
  <div class="py-4">
    <h3 class="text-lg font-bold text-white text-center mb-4">{{ title }}</h3>
    <div class="h-2 bg-sift-card rounded-full overflow-hidden mb-3">
      <div
        class="h-full rounded-full bg-linear-to-r from-sift-accent to-blue-400 transition-all duration-300 relative overflow-hidden"
        :style="{ width: `${percent}%` }"
      >
        <div class="absolute inset-0 shimmer-bar" />
      </div>
    </div>
    <p class="text-xs text-sift-muted text-center truncate">{{ currentFile }}</p>
    <p
      class="text-[11px] text-center mt-3"
      :class="isCloseBlocked ? 'text-sift-star' : 'text-sift-muted'"
      role="status"
    >
      {{ isCloseBlocked ? '正在整理照片，完成后才能关闭 Sift' : '整理完成前请不要关闭 Sift' }}
    </p>
  </div>
</template>
