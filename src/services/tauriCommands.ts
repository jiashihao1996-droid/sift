// ============================================================
// Sift - Tauri IPC Command Wrappers
// ============================================================

import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type {
  ArchivePairInput,
  DeletePairInput,
  DeleteResult,
  ScanResult,
  ExifData,
  ThumbnailResult,
  ArchiveResult,
  ArchiveProgress,
  ExportResult,
  PhotoSource,
  FolderProgress,
} from '@/types'

/** Scan a folder for JPG+RAW pairs */
export async function scanFolder(folderPath: string): Promise<ScanResult> {
  return invoke<ScanResult>('scan_folder', { folderPath })
}

/**
 * Move pairs to the system trash in batches, carrying on past failures; the
 * result lists each pair as trashed or failed. Files already gone count as
 * trashed.
 */
export async function deletePairs(pairs: DeletePairInput[]): Promise<DeleteResult> {
  return invoke<DeleteResult>('delete_pairs', { pairs })
}

/** Listen for delete progress events (pairs handled so far) */
export function onDeleteProgress(callback: (progress: ArchiveProgress) => void) {
  return listen<ArchiveProgress>('delete-progress', (event) => {
    callback(event.payload)
  })
}

/** Archive picks into 精选/ and unmarked RAWs into RAW/. Rejects with an `OrganizeError`. */
export async function archivePhotos(
  folderPath: string,
  pairs: ArchivePairInput[],
): Promise<ArchiveResult> {
  return invoke<ArchiveResult>('archive_photos', { folderPath, pairs })
}

/** Export starred photos to a target directory. Rejects with an `OrganizeError`. */
export async function exportPicks(
  pairs: { jpgPath: string; rawPath: string | null; source: PhotoSource; xmpPaths: string[] }[],
  targetFolder: string,
): Promise<ExportResult> {
  return invoke<ExportResult>('export_picks', { pairs, targetFolder })
}

/** Generate thumbnails and extract dominant colors */
export async function generateThumbnails(
  pairs: { id: string; jpgPath: string }[],
): Promise<ThumbnailResult[]> {
  return invoke<ThumbnailResult[]>('generate_thumbnails', { pairs })
}

/** Read EXIF metadata from a photo file */
export async function readExif(
  jpgPath: string,
  rawPath?: string | null,
  source?: PhotoSource,
): Promise<ExifData> {
  return invoke<ExifData>('read_exif', { jpgPath, rawPath, source })
}

/** Listen for archive progress events */
export function onArchiveProgress(callback: (progress: ArchiveProgress) => void) {
  return listen<ArchiveProgress>('archive-progress', (event) => {
    callback(event.payload)
  })
}

/** Show a file in the system file manager (Finder / Explorer) */
export async function showInFolder(path: string): Promise<void> {
  return invoke<void>('show_in_folder', { path })
}

/** Copy an image file to the system clipboard */
export async function copyImageToClipboard(path: string): Promise<void> {
  return invoke<void>('copy_image_to_clipboard', { path })
}

/** Listen for export progress events */
export function onExportProgress(callback: (progress: ArchiveProgress) => void) {
  return listen<ArchiveProgress>('export-progress', (event) => {
    callback(event.payload)
  })
}

/** Clean up temporary cache directories (sift-thumbnails, sift-raw-previews) */
export async function cleanupCache(): Promise<void> {
  return invoke<void>('cleanup_cache')
}

/** Saved culling progress for a folder, or null if there is none */
export async function loadProgress(folderPath: string): Promise<FolderProgress | null> {
  return invoke<FolderProgress | null>('load_progress', { folderPath })
}

/** Persist culling progress for `progress.folder` */
export async function saveProgress(progress: FolderProgress): Promise<void> {
  return invoke<void>('save_progress', { progress })
}

/** Forget the saved culling progress of a folder */
export async function clearProgress(folderPath: string): Promise<void> {
  return invoke<void>('clear_progress', { folderPath })
}
