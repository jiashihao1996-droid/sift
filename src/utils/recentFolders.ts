// ============================================================
// Sift - Recently opened folders (persisted in localStorage)
// ============================================================

export const MAX_RECENT_FOLDERS = 5
const STORAGE_KEY = 'sift.recentFolders'

/** Most recent first, without duplicates, capped at `max` */
export function addRecentFolder(
  list: readonly string[],
  path: string,
  max = MAX_RECENT_FOLDERS,
): string[] {
  return [path, ...list.filter((p) => p !== path)].slice(0, max)
}

export function removeRecentFolder(list: readonly string[], path: string): string[] {
  return list.filter((p) => p !== path)
}

/** Validate untrusted stored data; anything malformed yields an empty list */
export function parseRecentFolders(raw: string | null): string[] {
  if (!raw) return []
  try {
    const value: unknown = JSON.parse(raw)
    if (!Array.isArray(value)) return []
    return value
      .filter((p): p is string => typeof p === 'string' && p.length > 0)
      .slice(0, MAX_RECENT_FOLDERS)
  } catch {
    return []
  }
}

export function loadRecentFolders(): string[] {
  try {
    return parseRecentFolders(localStorage.getItem(STORAGE_KEY))
  } catch (e) {
    console.warn('[Sift] Failed to read recent folders', e)
    return []
  }
}

export function saveRecentFolders(list: readonly string[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(list))
  } catch (e) {
    console.warn('[Sift] Failed to save recent folders', e)
  }
}
