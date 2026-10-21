// ============================================================
// Sift - Type Definitions
// ============================================================

// Shapes shared with Rust are generated from src-tauri/src/models by ts-rs
// into ./generated — never edit those files; run `npm run gen:types`.
import type { PhotoStatus as GeneratedPhotoStatus } from './generated/PhotoStatus'

export type { AfArea } from './generated/AfArea'
export type { AfInfo } from './generated/AfInfo'
export type { AfSpot } from './generated/AfSpot'
export type { ArchivePairInput } from './generated/ArchivePairInput'
export type { ArchiveProgress } from './generated/ArchiveProgress'
export type { ArchiveResult } from './generated/ArchiveResult'
export type { CaptureTime } from './generated/CaptureTime'
export type { DeleteFailure } from './generated/DeleteFailure'
export type { DeletePairInput } from './generated/DeletePairInput'
export type { DeleteResult } from './generated/DeleteResult'
export type { Dimensions } from './generated/Dimensions'
export type { ExifData } from './generated/ExifData'
export type { ExportPairInput } from './generated/ExportPairInput'
export type { ExportResult } from './generated/ExportResult'
export type { FolderProgress } from './generated/FolderProgress'
export type { NoTrashReason } from './generated/NoTrashReason'
export type { OrganizeError } from './generated/OrganizeError'
export type { OrganizeErrorKind } from './generated/OrganizeErrorKind'
export type { PhotoPair } from './generated/PhotoPair'
export type { PhotoSource } from './generated/PhotoSource'
export type { ScanProgress } from './generated/ScanProgress'
export type { ScanResult } from './generated/ScanResult'
export type { SkippedFile } from './generated/SkippedFile'
export type { SkipReason } from './generated/SkipReason'
export type { ThumbnailInput } from './generated/ThumbnailInput'
export type { ThumbnailResult } from './generated/ThumbnailResult'

/** Photo pair status (values come from the generated Rust enum) */
export type PhotoStatus = GeneratedPhotoStatus

/** Named status values, e.g. `PhotoStatus.Starred` */
export const PhotoStatus = {
  Unprocessed: 'unprocessed',
  Starred: 'starred',
  Deleted: 'deleted',
} as const satisfies Record<string, GeneratedPhotoStatus>

// Compile-time guard: fails typecheck if Rust adds a status not listed above
type MissingStatuses = Exclude<GeneratedPhotoStatus, (typeof PhotoStatus)[keyof typeof PhotoStatus]>
const _allStatusesListed: [MissingStatuses] extends [never] ? true : MissingStatuses = true
void _allStatusesListed

/** A photo's status before an undoable change; ids, since indexes move when re-sorted */
export interface StatusChange {
  pairId: string
  previousStatus: PhotoStatus
}

/** One undo step: a mark key changes one photo, a burst decision several */
export interface UndoAction {
  /** The photo to return to */
  pairId: string
  changes: StatusChange[]
}

/** App page views */
export type AppView = 'welcome' | 'culling' | 'summary'
