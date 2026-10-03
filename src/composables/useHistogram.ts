// ============================================================
// Sift - Histogram of the current preview JPG
// Loads its own CORS-enabled copy of the image (the viewer's cached <img>s
// are no-cors and would taint the canvas), samples it downscaled, caches.
// ============================================================

import { ref, watch, type Ref } from 'vue'
import { convertFileSrc } from '@tauri-apps/api/core'
import { computeHistogram, type Histogram } from '@/core/histogram'

/** Long edge of the sampled image: enough to keep small blown areas, cheap to scan */
const SAMPLE_LONG_EDGE = 1600
const MAX_CACHE_SIZE = 50

const cache = new Map<string, Histogram>()

function remember(path: string, histogram: Histogram) {
  if (cache.size >= MAX_CACHE_SIZE) {
    const oldest = cache.keys().next().value
    if (oldest !== undefined) cache.delete(oldest)
  }
  cache.set(path, histogram)
}

async function sampleImage(path: string): Promise<Histogram> {
  const img = new Image()
  img.crossOrigin = 'anonymous'
  img.src = convertFileSrc(path)
  await img.decode()

  const scale = Math.min(1, SAMPLE_LONG_EDGE / Math.max(img.naturalWidth, img.naturalHeight))
  const width = Math.max(1, Math.round(img.naturalWidth * scale))
  const height = Math.max(1, Math.round(img.naturalHeight * scale))
  const canvas = document.createElement('canvas')
  canvas.width = width
  canvas.height = height
  const ctx = canvas.getContext('2d', { willReadFrequently: true })
  if (!ctx) throw new Error('Canvas 2D context unavailable')
  ctx.drawImage(img, 0, 0, width, height)
  return computeHistogram(ctx.getImageData(0, 0, width, height).data)
}

/** Histogram for `path`, computed only while `isEnabled` is true */
export function useHistogram(path: Ref<string | null>, isEnabled: Ref<boolean>) {
  const histogram = ref<Histogram | null>(null)
  const isLoading = ref(false)
  const hasError = ref(false)
  let generation = 0

  watch(
    [path, isEnabled],
    async ([p, enabled]) => {
      const thisGeneration = ++generation
      hasError.value = false
      if (!p || !enabled) {
        isLoading.value = false
        return
      }
      const cached = cache.get(p)
      if (cached) {
        histogram.value = cached
        isLoading.value = false
        return
      }
      histogram.value = null
      isLoading.value = true
      try {
        const result = await sampleImage(p)
        remember(p, result)
        if (thisGeneration !== generation) return
        histogram.value = result
      } catch (e) {
        if (thisGeneration !== generation) return
        console.error(`[Sift] Histogram failed for ${p}`, e)
        hasError.value = true
      } finally {
        if (thisGeneration === generation) isLoading.value = false
      }
    },
    { immediate: true },
  )

  return { histogram, isLoading, hasError }
}
