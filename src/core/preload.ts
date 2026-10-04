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

/**
 * `items` reordered by distance from `index`, the photo on screen first and
 * then alternating ahead and behind, so thumbnails appear where the user is
 * looking instead of from the start of the folder
 */
export function nearestFirst<T>(items: readonly T[], index: number): T[] {
  const start = Math.min(Math.max(index, 0), items.length - 1)
  const order: T[] = []
  for (let distance = 0; order.length < items.length; distance++) {
    const ahead = start + distance
    const behind = start - distance
    if (ahead < items.length) order.push(items[ahead])
    if (distance > 0 && behind >= 0) order.push(items[behind])
  }
  return order
}
