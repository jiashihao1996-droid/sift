// ============================================================
// Sift - Folders Without a Trash
// On a network share, or a FAT/exFAT card on Windows, deleting is permanent.
// The archive dialog says so and asks for confirmation every time.
// ============================================================

import type { NoTrashReason } from '@/types'

const NO_TRASH_TEXT: Record<NoTrashReason, string> = {
  networkVolume: '这个文件夹在网络共享（NAS / SMB）上，没有回收站。',
  removableFat: '这是 FAT / exFAT 格式的存储卡或 U 盘，Windows 不为它提供回收站。',
}

export function describeNoTrash(reason: NoTrashReason): string {
  return NO_TRASH_TEXT[reason]
}

/** Archiving would delete photos for good, so it must be confirmed first */
export function needsPermanentConfirm(reason: NoTrashReason | null, deletedCount: number): boolean {
  return reason !== null && deletedCount > 0
}
