// ============================================================
// Sift - Recently opened folders: list rules (storage lives in useRecentFolders)
// ============================================================

export const MAX_RECENT_FOLDERS = 5

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
