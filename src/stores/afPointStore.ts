// ============================================================
// Sift - Focus Point Store (Pinia)
// Where the camera focused, read on demand while the overlay is on and
// kept per photo. A photo costs one small read of its file's start.
// ============================================================

import { defineStore } from 'pinia'
import { reactive } from 'vue'
import type { AfInfo, PhotoPair } from '@/types'
import { readAfPoint } from '@/services/tauriCommands'

/** A few KB each; enough for a long session of browsing back and forth */
const MAX_ENTRIES = 500

export const useAfPointStore = defineStore('afPoint', () => {
  /** By pair id; ids are new on every scan, so a rescan reads afresh */
  const infos = reactive(new Map<string, AfInfo>())
  const inFlight = new Set<string>()

  /** Focus data of `pair`; undefined until it has been read */
  function infoOf(pair: PhotoPair | null | undefined): AfInfo | undefined {
    return pair ? infos.get(pair.id) : undefined
  }

  function remember(id: string, info: AfInfo) {
    infos.set(id, info)
    if (infos.size > MAX_ENTRIES) {
      const oldest = infos.keys().next().value
      if (oldest !== undefined) infos.delete(oldest)
    }
  }

  async function read(pair: PhotoPair) {
    inFlight.add(pair.id)
    try {
      remember(pair.id, await readAfPoint(pair.jpgPath, pair.rawPath, pair.source))
    } catch (e) {
      console.warn(`[Sift] Failed to read focus point of ${pair.jpgPath}:`, e)
      remember(pair.id, { kind: 'unavailable' })
    } finally {
      inFlight.delete(pair.id)
    }
  }

  /** Read the focus data of any of `pairs` not read yet, in order */
  function load(pairs: readonly (PhotoPair | null | undefined)[]) {
    for (const pair of pairs) {
      if (pair && !infos.has(pair.id) && !inFlight.has(pair.id)) void read(pair)
    }
  }

  return { infoOf, load }
})
