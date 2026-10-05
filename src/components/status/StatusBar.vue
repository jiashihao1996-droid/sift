<script setup lang="ts">
// ============================================================
// StatusBar - Bottom progress bar with rolling numbers
// ============================================================

import { computed } from 'vue'
import { ArrowDownUp } from 'lucide-vue-next'
import { useSessionStore } from '@/stores/sessionStore'
import { useViewStore } from '@/stores/viewStore'
import { PhotoStatus } from '@/types'
import { SortOrder, countWithoutCaptureTime } from '@/core/sorting'
import RollingNumber from './RollingNumber.vue'

const session = useSessionStore()
const view = useViewStore()

const progressWidth = computed(() => `${session.progress}%`)

const isByCaptureTime = computed(() => view.sortOrder === SortOrder.CaptureTime)

/** Only worth mentioning when sorting by capture time, which puts them last */
const untimedCount = computed(() =>
  isByCaptureTime.value ? countWithoutCaptureTime(session.pairs) : 0,
)

function toggleSortOrder() {
  view.setSortOrder(isByCaptureTime.value ? SortOrder.FileName : SortOrder.CaptureTime)
}

function handleStatusClick(status: PhotoStatus) {
  view.openFilterGallery(status)
}
</script>

<template>
  <div
    class="fixed bottom-0 left-0 right-0 h-10 z-50 bg-[#121212] border-t border-white/5 flex items-center px-4"
  >
    <!-- Left: Progress counter -->
    <div class="flex items-center gap-1.5 min-w-[100px]">
      <span class="text-sm font-mono text-white font-medium">
        <RollingNumber :value="session.currentIndex + 1" />
      </span>
      <span class="text-sm text-sift-muted">/</span>
      <span class="text-sm font-mono text-sift-muted">
        {{ session.totalCount }}
      </span>
      <button
        class="ml-3 h-6 flex items-center gap-1 rounded px-2 text-xs text-sift-muted hover:text-white hover:bg-white/5 transition-colors cursor-pointer"
        :title="
          isByCaptureTime
            ? '按拍摄时间排序（点击改为按文件名）'
            : '按文件名排序（点击改为按拍摄时间）'
        "
        @click="toggleSortOrder"
      >
        <ArrowDownUp :size="12" />
        {{ isByCaptureTime ? '拍摄时间' : '文件名' }}
      </button>
      <span
        v-if="untimedCount > 0"
        class="text-xs text-sift-muted/70 whitespace-nowrap"
        title="这些照片没有记录拍摄时间（EXIF），按文件名排在末尾"
      >
        {{ untimedCount }} 张无拍摄时间，排在末尾
      </span>
    </div>

    <!-- Center: Shimmer progress bar -->
    <div class="flex-1 mx-6 relative">
      <div class="h-1 bg-sift-card rounded-full overflow-hidden">
        <div
          class="h-full rounded-full bg-gradient-to-r from-sift-accent to-blue-400 transition-all duration-500 ease-out relative overflow-hidden"
          :style="{ width: progressWidth }"
        >
          <!-- Shimmer overlay -->
          <div class="absolute inset-0 shimmer-bar" />
        </div>
      </div>
    </div>

    <!-- Right: Status counts (clickable) -->
    <div class="flex items-center gap-4 min-w-[200px] justify-end">
      <!-- Stay after mark: only shown while on, click to switch back -->
      <button
        v-if="view.stayAfterMark"
        class="h-6 rounded px-2 text-xs text-sift-accent bg-sift-accent/10 hover:bg-sift-accent/20 transition-colors cursor-pointer"
        title="标记后停在当前照片（L 切换）"
        @click="view.toggleStayAfterMark()"
      >
        停留
      </button>

      <!-- Starred -->
      <button
        class="flex items-center gap-1.5 h-full rounded px-1 -mx-1 hover:bg-white/5 transition-colors cursor-pointer"
        @click="handleStatusClick(PhotoStatus.Starred)"
      >
        <span class="w-2 h-2 shrink-0 rounded-full bg-sift-star" />
        <RollingNumber :value="session.starredCount" class="text-xs text-sift-muted" />
      </button>

      <!-- Deleted -->
      <button
        class="flex items-center gap-1.5 h-full rounded px-1 -mx-1 hover:bg-white/5 transition-colors cursor-pointer"
        @click="handleStatusClick(PhotoStatus.Deleted)"
      >
        <span class="w-2 h-2 shrink-0 rounded-full bg-sift-delete" />
        <RollingNumber :value="session.deletedCount" class="text-xs text-sift-muted" />
      </button>

      <!-- Unmarked -->
      <button
        class="flex items-center gap-1.5 h-full rounded px-1 -mx-1 hover:bg-white/5 transition-colors cursor-pointer"
        @click="handleStatusClick(PhotoStatus.Unprocessed)"
      >
        <span class="w-2 h-2 shrink-0 rounded-full bg-sift-accent" />
        <RollingNumber :value="session.unprocessedCount" class="text-xs text-sift-muted" />
      </button>
    </div>
  </div>
</template>
