// ============================================================
// Sift - EXIF of the photos on screen
// One read per photo: results (and reads still running) are shared by the
// EXIF panel and the compare / survey panes.
// ============================================================

import { readExif } from '@/services/tauriCommands'
import type { ExifData, PhotoPair } from '@/types'

const MAX_CACHE_SIZE = 100

/** null: the photo has no readable EXIF */
const cache = new Map<string, ExifData | null>()
const inFlight = new Map<string, Promise<ExifData | null>>()

function keyOf(pair: PhotoPair): string {
  return `${pair.jpgPath}\n${pair.rawPath ?? ''}`
}

function remember(key: string, exif: ExifData | null) {
  if (cache.size >= MAX_CACHE_SIZE) {
    const oldest = cache.keys().next().value
    if (oldest !== undefined) cache.delete(oldest)
  }
  cache.set(key, exif)
}

/** EXIF already read for `pair`; undefined when not read yet */
export function cachedExif(pair: PhotoPair): ExifData | null | undefined {
  return cache.get(keyOf(pair))
}

/** EXIF of `pair`, or null when it has none; a failed read is logged, not thrown */
export function loadExif(pair: PhotoPair): Promise<ExifData | null> {
  const key = keyOf(pair)
  if (cache.has(key)) return Promise.resolve(cache.get(key) ?? null)
  const running = inFlight.get(key)
  if (running) return running
  const reading = readExif(pair.jpgPath, pair.rawPath, pair.source)
    .catch((e: unknown) => {
      console.warn(`[Sift] No EXIF for ${pair.jpgPath}`, e)
      return null
    })
    .then((exif) => {
      remember(key, exif)
      return exif
    })
    .finally(() => inFlight.delete(key))
  inFlight.set(key, reading)
  return reading
}
