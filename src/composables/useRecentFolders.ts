// ============================================================
// Sift - Recently opened folders, persisted in localStorage
// ============================================================

import { ref } from 'vue'
import { addRecentFolder, parseRecentFolders, removeRecentFolder } from '@/core/recentFolders'

const STORAGE_KEY = 'sift.recentFolders'

function load(): string[] {
  try {
    return parseRecentFolders(localStorage.getItem(STORAGE_KEY))
  } catch (e) {
    console.warn('[Sift] Failed to read recent folders', e)
    return []
  }
}

function save(list: readonly string[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(list))
  } catch (e) {
    console.warn('[Sift] Failed to save recent folders', e)
  }
}

export function useRecentFolders() {
  const folders = ref<string[]>(load())

  function update(next: string[]) {
    folders.value = next
    save(next)
  }

  return {
    folders,
    remember: (path: string) => update(addRecentFolder(folders.value, path)),
    forget: (path: string) => update(removeRecentFolder(folders.value, path)),
  }
}
