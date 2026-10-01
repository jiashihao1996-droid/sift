// ============================================================
// Sift - Error Utilities
// Tauri commands reject with a plain string (Rust `Result<_, String>`),
// while JS code throws `Error` objects — normalize both.
// ============================================================

/**
 * Extract a human-readable message from an unknown thrown value.
 */
export function getErrorMessage(error: unknown, fallback = '未知错误'): string {
  if (typeof error === 'string' && error.length > 0) return error
  if (error instanceof Error && error.message.length > 0) return error.message
  return fallback
}
