// ============================================================
// Sift - Organize Errors
// What the user is told when archiving or exporting stops partway: what
// went wrong in plain Chinese, what was already done, the file it got stuck
// on and what to do next. The original message stays available as details.
// ============================================================

import type { OrganizeError, OrganizeErrorKind } from '@/types'
import { extractFileName } from '@/utils/path'

const KIND_TEXT: Record<OrganizeErrorKind, { reason: string; advice: string }> = {
  permissionDenied: {
    reason: '没有权限访问这个文件或文件夹',
    advice:
      '确认 Sift 可以读写这个文件夹（macOS 在「系统设置 → 隐私与安全性 → 文件与文件夹」里允许），然后再试。',
  },
  diskFull: {
    reason: '磁盘空间不足',
    advice: '腾出一些空间后再试。',
  },
  readOnly: {
    reason: '这个位置是只读的',
    advice: '如果是 SD 卡，检查侧面的写保护开关是否拨到了 LOCK；也可以先把照片复制到电脑上再整理。',
  },
  notFound: {
    reason: '文件已经不在原来的位置',
    advice: '可能被其他程序移动、改名或删除了。返回首页重新打开文件夹，按现在的文件继续。',
  },
  trashUnsupported: {
    reason: '这个位置不支持回收站',
    advice:
      '标记删除的照片没有被删除。返回后在归档页勾选「永久删除」确认，或先把照片复制到电脑上再整理。',
  },
  automationDenied: {
    reason: 'Sift 没有控制「访达」的权限',
    advice:
      '把照片移到废纸篓需要这个权限：打开「系统设置 → 隐私与安全性 → 自动化」，允许 Sift 控制「访达」，然后再试。',
  },
  other: {
    reason: '出现了意外的错误',
    advice: '可以再试一次；如果仍然失败，请展开「详情」，把内容发给开发者。',
  },
}

const KINDS = Object.keys(KIND_TEXT) as OrganizeErrorKind[]

function isKind(value: unknown): value is OrganizeErrorKind {
  return typeof value === 'string' && (KINDS as string[]).includes(value)
}

/**
 * Narrow what an organize command (archive, export, delete) rejected with.
 * Anything else that was thrown, such as a plain string, becomes `other`.
 */
export function toOrganizeError(error: unknown): OrganizeError {
  if (typeof error === 'object' && error !== null) {
    const e = error as Record<string, unknown>
    if (isKind(e.kind) && typeof e.detail === 'string') {
      return {
        kind: e.kind,
        path: typeof e.path === 'string' ? e.path : null,
        detail: e.detail,
        completed: typeof e.completed === 'number' ? e.completed : 0,
      }
    }
  }
  const detail =
    typeof error === 'string' ? error : error instanceof Error ? error.message : String(error)
  return { kind: 'other', path: null, detail: detail || '未知错误', completed: 0 }
}

/** Where an archive, a delete-only run or an export was when it stopped */
export type OrganizeStage =
  | {
      /** `delete`: only the photos marked for deletion, nothing else moves */
      operation: 'archive' | 'delete'
      step: 'delete'
      deletedGroups: number
      failedGroups: number
      /** Deleted for good, in a folder without a trash */
      isPermanent?: boolean
    }
  | { operation: 'archive'; step: 'move'; deletedGroups: number; isPermanent?: boolean }
  | { operation: 'export'; folder: string }

export interface FailureReport {
  title: string
  reason: string
  /** What already happened, one line each; never empty */
  done: string[]
  /** Name of the file it got stuck on, when known */
  stuckFile: string | null
  stuckPath: string | null
  advice: string
  /** The original message, for the details view */
  detail: string
  /**
   * True when the session still matches the files on disk, so trying again
   * is safe; false once photos have moved, which only a rescan picks up
   */
  canRetry: boolean
}

/** What happened to the deleted photos, for the done list and success notices */
export function describeDeleted(groups: number, isPermanent = false): string {
  return isPermanent ? `已永久删除 ${groups} 组照片` : `已把 ${groups} 组照片移到回收站`
}

function doneLines(stage: OrganizeStage, completed: number): string[] {
  if (stage.operation === 'export') {
    return [
      completed > 0 ? `已导出 ${completed} 张照片到 ${stage.folder}` : '还没有导出任何照片',
      '原文件夹里的照片没有任何改动',
    ]
  }
  const lines: string[] = []
  if (stage.deletedGroups > 0) lines.push(describeDeleted(stage.deletedGroups, stage.isPermanent))
  if (stage.step === 'delete') {
    lines.push(`${stage.failedGroups} 组没有删完，仍标记为删除`)
    lines.push(stage.operation === 'archive' ? '还没有开始归档' : '其他照片没有任何改动')
  } else if (completed > 0) {
    lines.push(`已归档 ${completed} 张照片`)
  } else if (stage.deletedGroups === 0) {
    lines.push('还没有改动任何文件')
  }
  return lines
}

function canRetry(stage: OrganizeStage, completed: number): boolean {
  if (stage.operation === 'export' || stage.step === 'delete') return true
  return completed === 0
}

const TITLES: Record<OrganizeStage['operation'], string> = {
  archive: '归档没有完成',
  delete: '删除没有完成',
  export: '导出没有完成',
}

export function describeFailure(stage: OrganizeStage, error: OrganizeError): FailureReport {
  const { reason, advice } = KIND_TEXT[error.kind]
  const isRetryable = canRetry(stage, error.completed)
  return {
    title: TITLES[stage.operation],
    reason,
    done: doneLines(stage, error.completed),
    stuckFile: error.path ? extractFileName(error.path) : null,
    stuckPath: error.path,
    advice:
      isRetryable || error.kind === 'notFound'
        ? advice
        : `${advice}已完成的部分不会撤销，返回首页重新打开文件夹，可以接着整理剩下的照片。`,
    detail: error.detail,
    canRetry: isRetryable,
  }
}
