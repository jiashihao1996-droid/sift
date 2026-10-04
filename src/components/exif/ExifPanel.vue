<script setup lang="ts">
// ============================================================
// ExifPanel - Right slide-in drawer with EXIF metadata
// ============================================================

import { computed, ref, watch } from 'vue'
import { X, Camera, Aperture, Clock, ChartArea } from '@lucide/vue'
import { useSessionStore } from '@/stores/sessionStore'
import { useViewStore } from '@/stores/viewStore'
import { readExif } from '@/services/tauriCommands'
import type { ExifData } from '@/types'
import { useHistogram } from '@/composables/useHistogram'
import HistogramView from './HistogramView.vue'

const session = useSessionStore()
const view = useViewStore()

const exifData = ref<ExifData | null>(null)
const isLoading = ref(false)

const histogramState = useHistogram(
  computed(() => session.currentPair?.jpgPath ?? null),
  computed(() => view.showExifPanel),
)

const histogramSource = computed(() => {
  const pair = session.currentPair
  if (pair?.source === 'rawPreview') return '基于 RAW 内嵌预览；RAW 本身通常还有高光余量'
  if (pair?.rawPath) return '基于 JPG；对应的 RAW 通常还有高光余量'
  return '基于 JPG'
})

function formatFileSize(bytes: number): string {
  if (bytes === 0) return '-'
  if (bytes >= 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  }
  return `${(bytes / 1024).toFixed(1)} KB`
}

watch(
  [() => session.currentPair, () => view.showExifPanel],
  async ([pair, show]) => {
    if (!show || !pair) {
      return
    }
    isLoading.value = true
    try {
      exifData.value = await readExif(pair.jpgPath, pair.rawPath, pair.source)
    } catch {
      exifData.value = null
    } finally {
      isLoading.value = false
    }
  },
  { immediate: true },
)
</script>

<template>
  <Transition name="exif">
    <div
      v-if="view.showExifPanel"
      class="fixed top-12 right-0 bottom-10 w-72 z-40 bg-sift-surface/95 backdrop-blur-xl border-l border-sift-border overflow-y-auto"
    >
      <!-- Header -->
      <div class="flex items-center justify-between px-4 py-3 border-b border-sift-border">
        <span class="text-sm font-semibold text-white">拍摄信息</span>
        <button
          class="p-1 rounded hover:bg-white/10 transition-colors btn-spring"
          @click="view.toggleExifPanel()"
        >
          <X :size="14" class="text-sift-muted" />
        </button>
      </div>

      <!-- Histogram (independent of EXIF loading) -->
      <section class="p-4 border-b border-sift-border">
        <div class="flex items-center gap-2 mb-2">
          <ChartArea :size="14" class="text-sift-accent" />
          <span class="text-xs text-sift-muted uppercase tracking-wider">直方图</span>
        </div>
        <HistogramView
          :histogram="histogramState.histogram.value"
          :is-loading="histogramState.isLoading.value"
          :has-error="histogramState.hasError.value"
          :source-label="histogramSource"
        />
      </section>

      <!-- Loading -->
      <div v-if="isLoading" class="p-4 space-y-3">
        <div class="skeleton-pulse h-4 rounded w-3/4" />
        <div class="skeleton-pulse h-4 rounded w-1/2" />
        <div class="skeleton-pulse h-4 rounded w-2/3" />
      </div>

      <!-- No data -->
      <div v-else-if="!exifData" class="p-4 text-center">
        <p class="text-sift-muted text-sm">暂无 EXIF 数据</p>
      </div>

      <!-- EXIF Content -->
      <div v-else class="p-4 space-y-5">
        <!-- Camera & Lens -->
        <div class="space-y-2">
          <div class="flex items-center gap-2 mb-2">
            <Camera :size="14" class="text-sift-accent" />
            <span class="text-xs text-sift-muted uppercase tracking-wider">相机</span>
          </div>
          <p class="text-sm text-white font-medium">{{ exifData.camera || '未知' }}</p>
          <p class="text-xs text-sift-muted">{{ exifData.lens || '未知镜头' }}</p>
        </div>

        <div class="h-px bg-sift-border" />

        <!-- Exposure Grid -->
        <div class="space-y-2">
          <div class="flex items-center gap-2 mb-2">
            <Aperture :size="14" class="text-sift-accent" />
            <span class="text-xs text-sift-muted uppercase tracking-wider">曝光</span>
          </div>
          <div class="grid grid-cols-3 gap-3">
            <div>
              <p class="text-[11px] text-sift-muted">光圈</p>
              <p class="text-sm text-white font-semibold">{{ exifData.aperture || '-' }}</p>
            </div>
            <div>
              <p class="text-[11px] text-sift-muted">快门</p>
              <p class="text-sm text-white font-semibold">{{ exifData.shutterSpeed || '-' }}</p>
            </div>
            <div>
              <p class="text-[11px] text-sift-muted">ISO</p>
              <p class="text-sm text-white font-semibold">{{ exifData.iso || '-' }}</p>
            </div>
          </div>
          <div>
            <p class="text-[11px] text-sift-muted">焦距</p>
            <p class="text-sm text-white font-semibold">{{ exifData.focalLength || '-' }}</p>
          </div>
        </div>

        <div class="h-px bg-sift-border" />

        <!-- Time & Dimensions -->
        <div class="space-y-2">
          <div class="flex items-center gap-2 mb-2">
            <Clock :size="14" class="text-sift-accent" />
            <span class="text-xs text-sift-muted uppercase tracking-wider">详情</span>
          </div>
          <div class="space-y-2">
            <div>
              <p class="text-[11px] text-sift-muted">拍摄时间</p>
              <p class="text-sm text-white">{{ exifData.dateTaken || '-' }}</p>
            </div>
            <div>
              <p class="text-[11px] text-sift-muted">尺寸</p>
              <p class="text-sm text-white">
                {{ exifData.dimensions?.width || '?' }} × {{ exifData.dimensions?.height || '?' }}
              </p>
            </div>
            <div>
              <p class="text-[11px] text-sift-muted">文件大小</p>
              <p class="text-sm text-white">{{ formatFileSize(exifData.fileSize) }}</p>
            </div>
          </div>
        </div>
      </div>
    </div>
  </Transition>
</template>
