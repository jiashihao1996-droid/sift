// ============================================================
// Sift - Resolve what to scan from a drag-and-drop payload
// ============================================================

/** Mirrors JPG_EXTENSIONS + RAW_EXTENSIONS in the Rust file_types.rs */
// prettier-ignore
const PHOTO_EXTENSIONS = new Set([
  'jpg', 'jpeg',
  'cr2', 'cr3', 'crw', 'nef', 'nrw', 'arw', 'srf', 'sr2', 'raf', 'orf', 'rw2', 'pef',
  'rwl', '3fr', 'fff', 'iiq', 'srw', 'x3f', 'kdc', 'dcr', 'erf', 'mef', 'mos', 'dng',
])

function isPhotoFile(path: string): boolean {
  const name = path.split(/[\\/]/).pop() ?? ''
  const dot = name.lastIndexOf('.')
  return dot > 0 && PHOTO_EXTENSIONS.has(name.slice(dot + 1).toLowerCase())
}

/** Parent directory, keeping the root separator ("/a" → "/", "C:\a" → "C:\") */
export function parentDir(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, '')
  const cut = Math.max(trimmed.lastIndexOf('/'), trimmed.lastIndexOf('\\'))
  if (cut < 0) return trimmed
  const parent = trimmed.slice(0, cut)
  return parent === '' || parent.endsWith(':') ? trimmed.slice(0, cut + 1) : parent
}

/**
 * Folder to scan for dropped paths: a dropped photo opens its folder, anything
 * else is treated as a folder (the scan rejects non-directories).
 */
export function resolveDropTarget(paths: readonly string[]): string | null {
  const first = paths[0]
  if (!first) return null
  return isPhotoFile(first) ? parentDir(first) : first
}
