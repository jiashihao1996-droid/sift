// ============================================================
// Sift - Preload rules: which photos to load ahead of time
// Pure — no Vue, Pinia or Tauri imports.
// ============================================================

/** Photos loaded ahead in the direction of travel; culling mostly moves forward */
const AHEAD = 3
/** Photos kept warm behind, for a quick look back */
const BEHIND = 1

/**
 * Indices to preload around `index`, nearest first: `AHEAD` photos in the
 * direction of travel (`step` is +1 forward, -1 backward), then `BEHIND` the
 * other way. Stays within `0..total`.
 */
export function preloadOrder(index: number, total: number, step: 1 | -1): number[] {
  const ahead = Array.from({ length: AHEAD }, (_, i) => index + step * (i + 1))
  const behind = Array.from({ length: BEHIND }, (_, i) => index - step * (i + 1))
  return [...ahead, ...behind].filter((i) => i >= 0 && i < total)
}
