// ============================================================
// Sift - Archive Flow
// The order of an archive: trash every pair marked for deletion in one
// batch, drop the trashed pairs from the session, and only when none failed
// sort the rest into 精选/ and RAW/. Commands are injected so the flow is
// tested without Tauri.
// ============================================================

import {
  PhotoStatus,
  type ArchivePairInput,
  type ArchiveResult,
  type DeletePairInput,
  type DeleteResult,
  type PhotoPair,
} from '@/types'
import { describeFailure, toOrganizeError, type FailureReport } from './organizeError'

export interface ArchiveDeps {
  deletePairs(pairs: DeletePairInput[]): Promise<DeleteResult>
  archivePhotos(folder: string, pairs: ArchivePairInput[]): Promise<ArchiveResult>
  /** Remove pairs whose files are gone from the session */
  dropPairs(ids: readonly string[]): void
}

export type ArchiveOutcome =
  { ok: true; deletedGroups: number; movedCount: number } | { ok: false; report: FailureReport }

function toDeleteInput(pair: PhotoPair): DeletePairInput {
  return {
    id: pair.id,
    jpgPath: pair.jpgPath,
    rawPath: pair.rawPath,
    source: pair.source,
    xmpPaths: pair.xmpPaths,
  }
}

function toArchiveInput(pair: PhotoPair): ArchivePairInput {
  return {
    jpgPath: pair.jpgPath,
    rawPath: pair.rawPath,
    status: pair.status,
    source: pair.source,
    xmpPaths: pair.xmpPaths,
  }
}

/**
 * Delete, then archive. Trashed pairs leave the session right away, so a
 * failed delete can simply be retried: only the pairs still marked are sent
 * again, and a pair that is partly in the trash finishes.
 */
export async function runArchive(
  folder: string,
  pairs: readonly PhotoPair[],
  deps: ArchiveDeps,
): Promise<ArchiveOutcome> {
  const toDelete = pairs.filter((p) => p.status === PhotoStatus.Deleted)
  let deletedGroups = 0

  if (toDelete.length > 0) {
    let result: DeleteResult
    try {
      result = await deps.deletePairs(toDelete.map(toDeleteInput))
    } catch (e) {
      const stage = {
        operation: 'archive',
        step: 'delete',
        deletedGroups,
        failedGroups: toDelete.length,
      } as const
      return { ok: false, report: describeFailure(stage, toOrganizeError(e)) }
    }
    deps.dropPairs(result.trashed)
    deletedGroups = result.trashed.length
    const [firstFailure] = result.failed
    if (firstFailure) {
      const failedGroups = result.failed.length
      const stage = { operation: 'archive', step: 'delete', deletedGroups, failedGroups } as const
      return { ok: false, report: describeFailure(stage, firstFailure.error) }
    }
  }

  const survivors = pairs.filter((p) => p.status !== PhotoStatus.Deleted).map(toArchiveInput)
  try {
    const result = await deps.archivePhotos(folder, survivors)
    return { ok: true, deletedGroups, movedCount: result.movedCount }
  } catch (e) {
    const stage = { operation: 'archive', step: 'move', deletedGroups } as const
    return { ok: false, report: describeFailure(stage, toOrganizeError(e)) }
  }
}

/**
 * The session without the pairs in `ids`. The current photo stays current
 * when it survives; otherwise the next surviving photo takes its place.
 */
export function withoutPairs(
  pairs: readonly PhotoPair[],
  currentIndex: number,
  ids: readonly string[],
): { pairs: PhotoPair[]; currentIndex: number } {
  const gone = new Set(ids)
  const kept = pairs.filter((p) => !gone.has(p.id))
  const keptBefore = pairs.slice(0, currentIndex).filter((p) => !gone.has(p.id)).length
  return { pairs: kept, currentIndex: Math.min(keptBefore, Math.max(kept.length - 1, 0)) }
}
