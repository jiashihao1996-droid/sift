// ============================================================
// Sift - Scan Report
// How files the scan could not load are presented to the user.
// ============================================================

import type { SkipReason, SkippedFile } from '@/types'
import { extractFileName } from '@/utils/path'

const SKIP_REASON_LABELS: Record<SkipReason, string> = {
  noPreview: '无法提取内嵌预览',
}

export interface SkippedLine {
  name: string
  path: string
  reason: string
}

/** One display line per skipped file, naturally sorted by file name */
export function describeSkipped(skipped: readonly SkippedFile[]): SkippedLine[] {
  return skipped
    .map((file) => ({
      name: extractFileName(file.path),
      path: file.path,
      reason: SKIP_REASON_LABELS[file.reason],
    }))
    .sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }))
}
