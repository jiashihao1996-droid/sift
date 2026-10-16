// ============================================================
// Sift - Folders Without a Trash
// On a network share, or a FAT/exFAT card on Windows, deleting is permanent.
// The archive dialog says so and asks for confirmation every time.
// ============================================================

import { PhotoStatus, type NoTrashReason, type PhotoPair } from '@/types'
import { extractFileName } from '@/utils/path'

const NO_TRASH_TEXT: Record<NoTrashReason, string> = {
  networkVolume: '这个文件夹在网络共享（NAS / SMB）上，没有回收站。',
  removableFat: '这是 FAT / exFAT 格式的存储卡或 U 盘，Windows 不为它提供回收站。',
}

export function describeNoTrash(reason: NoTrashReason): string {
  return NO_TRASH_TEXT[reason]
}

/** Archiving would delete files for good, so it must be confirmed first */
export function needsPermanentConfirm(reason: NoTrashReason | null, deletedCount: number): boolean {
  return reason !== null && deletedCount > 0
}

function stem(path: string): string {
  const name = extractFileName(path)
  const dot = name.lastIndexOf('.')
  return dot > 0 ? name.slice(0, dot) : name
}

/**
 * An unstarred pick from 精选/ whose JPG may be the preview an earlier
 * archive saved for its RAW: archiving drops that preview. Rust decides by
 * comparing the bytes; this only has to cover every case it might drop, so
 * the dialog asks for confirmation where deleting is permanent.
 */
export function mayDropSavedPreview(pair: PhotoPair): boolean {
  return (
    pair.isArchivedPick &&
    pair.status === PhotoStatus.Unprocessed &&
    pair.source === 'jpg' &&
    pair.rawPath !== null &&
    stem(pair.jpgPath) === stem(pair.rawPath)
  )
}
