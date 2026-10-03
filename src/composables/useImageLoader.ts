// ============================================================
// Sift - Image Loader Composable
// Progressive loading: skeleton -> thumbnail -> full image
// Preloads ahead in the direction of travel (see core/preload.ts)
// ============================================================

import { ref, watch } from 'vue'
import { convertFileSrc } from '@tauri-apps/api/core'
import { useSessionStore } from '@/stores/sessionStore'
import { preloadOrder } from '@/core/preload'
import { LruCache } from '@/utils/lruCache'

/** Timeout for loading a single image (ms) */
const LOAD_TIMEOUT = 30_000 // 30s for large RAW JPGs
/**
 * Decoded full-size photos are large (a 24MP JPEG is ~100MB as pixels), so keep
 * only what preloading needs plus a few recent ones
 */
const MAX_CACHE_SIZE = 10

export function useImageLoader() {
  const session = useSessionStore()

  const isLoading = ref(true)
  const currentSrc = ref('')
  const thumbnailSrc = ref('')
  const loadError = ref(false)
  const naturalWidth = ref(0)
  const naturalHeight = ref(0)

  // Image cache (only store successfully loaded images)
  const imageCache = new LruCache<string, HTMLImageElement>(MAX_CACHE_SIZE)

  // Abort controller for cancelling outdated loads
  let loadGeneration = 0

  /** Convert a local file path to a src usable in <img> or Canvas */
  function toSrc(path: string): string {
    return convertFileSrc(path)
  }

  /** Preload an image with timeout */
  function preloadImage(path: string): Promise<HTMLImageElement> {
    // Return cached image if available
    const cached = imageCache.get(path)
    if (cached) return Promise.resolve(cached)

    return new Promise((resolve, reject) => {
      const img = new Image()
      let settled = false

      const timer = setTimeout(() => {
        if (!settled) {
          settled = true
          img.src = '' // Cancel the load
          console.warn(`[Sift] Image load timeout (${LOAD_TIMEOUT}ms): ${path}`)
          reject(new Error(`Load timeout: ${path}`))
        }
      }, LOAD_TIMEOUT)

      img.onload = () => {
        if (!settled) {
          settled = true
          clearTimeout(timer)
          // Decode now, off the main thread, so showing the photo later does not
          // stall a frame; a failed decode still leaves a usable image
          void img
            .decode()
            .catch(() => {})
            .finally(() => {
              imageCache.set(path, img)
              resolve(img)
            })
        }
      }

      img.onerror = (event) => {
        if (!settled) {
          settled = true
          clearTimeout(timer)
          console.error(`[Sift] Image load error: ${path}`, event)
          reject(new Error(`Failed to load: ${path}`))
        }
      }

      img.src = toSrc(path)
    })
  }

  /** +1 when browsing forward, -1 backward; decides which way to preload */
  let travel: 1 | -1 = 1

  /** Load the current image with progressive display */
  async function loadCurrentImage() {
    const pair = session.currentPair
    if (!pair) {
      currentSrc.value = ''
      return
    }

    // Increment generation to cancel outdated loads
    const thisGeneration = ++loadGeneration

    isLoading.value = true
    loadError.value = false
    currentSrc.value = ''

    // Show thumbnail immediately if available
    if (pair.thumbnailPath) {
      thumbnailSrc.value = toSrc(pair.thumbnailPath)
    } else {
      thumbnailSrc.value = ''
    }

    try {
      // Load full image
      const img = await preloadImage(pair.jpgPath)

      // Only apply if this is still the current load
      if (thisGeneration !== loadGeneration) return

      currentSrc.value = toSrc(pair.jpgPath)
      naturalWidth.value = img.naturalWidth
      naturalHeight.value = img.naturalHeight
      isLoading.value = false

      // Preload the photos likely to come next
      preloadAround()
    } catch (e) {
      // Only apply error if this is still the current load
      if (thisGeneration !== loadGeneration) return

      console.error('[Sift] loadCurrentImage failed:', e)
      loadError.value = true
      isLoading.value = false
    }
  }

  /** Warm the cache with the photos likely to be shown next */
  function preloadAround() {
    for (const i of preloadOrder(session.currentIndex, session.pairs.length, travel)) {
      const path = session.pairs[i]?.jpgPath
      if (path) preloadImage(path).catch(() => {})
    }
  }

  /** Retry loading the current image (clears cache for this path) */
  function retryLoad() {
    const pair = session.currentPair
    if (pair) {
      // Remove from cache to force reload
      imageCache.delete(pair.jpgPath)
      loadCurrentImage()
    }
  }

  // Watch for index changes
  watch(
    () => session.currentIndex,
    (index, previous) => {
      if (index !== previous) travel = index > previous ? 1 : -1
      loadCurrentImage()
    },
  )

  // Initial load (immediate: true ensures first image loads when component mounts with existing data)
  watch(
    () => session.pairs.length,
    (len) => {
      if (len > 0) {
        loadCurrentImage()
      }
    },
    { immediate: true },
  )

  return {
    isLoading,
    currentSrc,
    thumbnailSrc,
    loadError,
    naturalWidth,
    naturalHeight,
    toSrc,
    loadCurrentImage,
    retryLoad,
  }
}
