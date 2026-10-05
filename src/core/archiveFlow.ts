// ============================================================
// Sift - Archive Flow
// The order of an archive: trash every pair marked for deletion in one
// batch, drop the trashed pairs from the session, and only when none failed
// sort the rest into 精选/ and RAW/. A delete-only run stops after the
// first step and moves nothing. Commands are injected so the flow is tested
// without Tauri.
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

export type DeleteOnlyOutcome =
  { ok: true; deletedGroups: number } | { ok: false; report: FailureReport }

export interface FlowOptions {
  /** The user confirmed that deleting is permanent here (a folder without a trash) */
  isPermanent?: boolean
}

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
 * Delete every pair marked for deletion in one batch. Trashed pairs leave
 * the session right away, so a failed delete can simply be retried: only
 * the pairs still marked are sent again, and a partly trashed pair finishes.
 */
async function deleteMarked(
  operation: 'archive' | 'delete',
  pairs: readonly PhotoPair[],
  deps: Pick<ArchiveDeps, 'deletePairs' | 'dropPairs'>,
  isPermanent: boolean,
): Promise<DeleteOnlyOutcome> {
  const toDelete = pairs.filter((p) => p.status === PhotoStatus.Deleted)
  if (toDelete.length === 0) return { ok: true, deletedGroups: 0 }
  const stage = { operation, step: 'delete', isPermanent } as const

  let result: DeleteResult
  try {
    result = await deps.deletePairs(toDelete.map(toDeleteInput))
  } catch (e) {
    const failed = { ...stage, deletedGroups: 0, failedGroups: toDelete.length }
    return { ok: false, report: describeFailure(failed, toOrganizeError(e)) }
  }
  deps.dropPairs(result.trashed)
  const deletedGroups = result.trashed.length
  const [firstFailure] = result.failed
  if (firstFailure) {
    const failed = { ...stage, deletedGroups, failedGroups: result.failed.length }
    return { ok: false, report: describeFailure(failed, firstFailure.error) }
  }
  return { ok: true, deletedGroups }
}

/** Delete, then archive; nothing moves while any marked pair is left */
export async function runArchive(
  folder: string,
  pairs: readonly PhotoPair[],
  deps: ArchiveDeps,
  { isPermanent = false }: FlowOptions = {},
): Promise<ArchiveOutcome> {
  const deleted = await deleteMarked('archive', pairs, deps, isPermanent)
  if (!deleted.ok) return deleted
  const { deletedGroups } = deleted

  const survivors = pairs.filter((p) => p.status !== PhotoStatus.Deleted).map(toArchiveInput)
  try {
    const result = await deps.archivePhotos(folder, survivors)
    return { ok: true, deletedGroups, movedCount: result.movedCount }
  } catch (e) {
    const stage = { operation: 'archive', step: 'move', deletedGroups, isPermanent } as const
    return { ok: false, report: describeFailure(stage, toOrganizeError(e)) }
  }
}

/** Only delete the pairs marked for deletion; every other file stays put */
export async function runDeleteOnly(
  pairs: readonly PhotoPair[],
  deps: Pick<ArchiveDeps, 'deletePairs' | 'dropPairs'>,
  { isPermanent = false }: FlowOptions = {},
): Promise<DeleteOnlyOutcome> {
  return deleteMarked('delete', pairs, deps, isPermanent)
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
