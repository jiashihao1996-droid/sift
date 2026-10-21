// ============================================================
// Sift - Scan Progress
// What the loading page says while a folder is scanned. Only RAW-only
// photos report progress: extracting their previews is the slow part.
// ============================================================

import type { ScanProgress } from '@/types'

export interface ScanStatus {
  label: string
  /** 0–100 while previews are extracted; null when there is no count to show */
  percent: number | null
}

export function describeScanProgress(progress: ScanProgress | null): ScanStatus {
  if (!progress || progress.total === 0) return { label: '正在扫描文件...', percent: null }
  const { done, total } = progress
  const percent = Math.floor((done / total) * 100)
  if (done >= total) return { label: '预览已提取，正在读取拍摄时间...', percent: 100 }
  return { label: `正在提取 RAW 预览 ${done} / ${total}`, percent }
}
