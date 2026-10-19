<script setup lang="ts">
// ============================================================
// StatusBadge - Shows a photo's mark on the viewer; nothing when unmarked
// ============================================================

import { Star, Trash2 } from '@lucide/vue'
import { PhotoStatus } from '@/types'

defineProps<{ status?: PhotoStatus }>()
</script>

<template>
  <span
    v-if="status === PhotoStatus.Starred"
    class="inline-flex items-center gap-1 px-2 py-1 rounded-lg bg-sift-star/90 text-black text-[11px] font-medium shadow-sm"
  >
    <Star :size="12" class="fill-black" />
    精选
  </span>
  <span
    v-else-if="status === PhotoStatus.Deleted"
    class="inline-flex items-center gap-1 px-2 py-1 rounded-lg bg-sift-delete/90 text-white text-[11px] font-medium shadow-sm"
  >
    <Trash2 :size="12" />
    删除
  </span>
</template>
