<script setup lang="ts">
// ============================================================
// ActionBar - Floating action buttons (Star / Delete)
// ============================================================

import { Star, Trash2 } from 'lucide-vue-next'
import { useSessionStore } from '@/stores/sessionStore'
import { ref, computed, watch } from 'vue'
import { PhotoStatus } from '@/types'
import type { MarkType } from '@/core/review'

const PULSE_MS = 300

const session = useSessionStore()

/** Button that just fired, by mouse or keyboard */
const pulsing = ref<MarkType | null>(null)
let pulseTimer: ReturnType<typeof setTimeout> | undefined

const isStarred = computed(() => session.currentPair?.status === PhotoStatus.Starred)
const isDeleted = computed(() => session.currentPair?.status === PhotoStatus.Deleted)

watch(
  () => session.lastMark,
  (change) => {
    if (!change) return
    pulsing.value = change.type
    clearTimeout(pulseTimer)
    pulseTimer = setTimeout(() => (pulsing.value = null), PULSE_MS)
  },
)
</script>

<template>
  <div
    class="absolute bottom-[120px] left-1/2 -translate-x-1/2 z-40 px-2 py-1.5 rounded-xl bg-black/50 backdrop-blur-2xl shadow-[0_8px_32px_rgba(0,0,0,0.4)] border border-white/10 flex items-center gap-1"
  >
    <!-- Star Button -->
    <button
      class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg group btn-spring transition-colors"
      :class="isStarred ? 'bg-sift-star/15 hover:bg-sift-star/25' : 'hover:bg-sift-star/15'"
      @click="session.markStar()"
    >
      <Star
        :size="16"
        class="transition-all"
        :class="[
          pulsing === 'star' ? 'scale-125' : '',
          isStarred ? 'text-sift-star fill-sift-star' : 'text-sift-star',
        ]"
      />
      <span
        class="text-[11px] group-hover:text-white transition-colors drop-shadow-[0_1px_2px_rgba(0,0,0,0.8)]"
        :class="isStarred ? 'text-sift-star' : 'text-white/60'"
        >{{ isStarred ? '取消精选' : '精选' }}</span
      >
      <kbd class="text-[9px] text-white/30 bg-white/[0.06] px-1 rounded">F</kbd>
    </button>

    <div class="w-px h-4 bg-white/10" />

    <!-- Delete Button -->
    <button
      class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg group btn-spring transition-colors"
      :class="isDeleted ? 'bg-sift-delete/15 hover:bg-sift-delete/25' : 'hover:bg-sift-delete/15'"
      @click="session.markDelete()"
    >
      <Trash2
        :size="16"
        class="text-sift-delete transition-all"
        :class="pulsing === 'delete' ? 'scale-125' : ''"
      />
      <span
        class="text-[11px] group-hover:text-white transition-colors drop-shadow-[0_1px_2px_rgba(0,0,0,0.8)]"
        :class="isDeleted ? 'text-sift-delete' : 'text-white/60'"
        >{{ isDeleted ? '取消删除' : '删除' }}</span
      >
      <kbd class="text-[9px] text-white/30 bg-white/[0.06] px-1 rounded">X</kbd>
    </button>
  </div>
</template>
