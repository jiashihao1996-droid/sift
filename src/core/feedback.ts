// ============================================================
// Sift - Mark feedback rules: what the toast says after a mark key
// Pure — no Vue, Pinia or Tauri imports.
// ============================================================

import type { PhotoPair } from '@/types'
import { extractFileName } from '@/utils/path'
import type { MarkType } from './review'

export type MarkToastType = 'star' | 'delete' | 'info'

/** One press of a mark key: which key, and whether the photo ended up marked */
export interface MarkChange {
  type: MarkType
  isMarked: boolean
}

/** File name to show for a photo: its RAW for RAW-only pairs, whose JPG is a cache file */
export function photoName(pair: PhotoPair): string {
  const path = pair.source === 'rawPreview' && pair.rawPath ? pair.rawPath : pair.jpgPath
  return extractFileName(path)
}

const MARKED: Record<MarkType, string> = { star: '★ 精选', delete: '标记删除' }
const UNMARKED: Record<MarkType, string> = { star: '已取消精选', delete: '已取消删除' }

/** Toast confirming a mark change on the photo called `name` */
export function markToast(
  change: MarkChange,
  name: string,
): { message: string; type: MarkToastType } {
  const label = (change.isMarked ? MARKED : UNMARKED)[change.type]
  return { message: `${label} · ${name}`, type: change.isMarked ? change.type : 'info' }
}
