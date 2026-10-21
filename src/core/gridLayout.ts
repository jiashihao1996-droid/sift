// ============================================================
// Sift - Gallery grid layout
// Columns, row height and the rows in view for the filter gallery's
// auto-filling grid, so it can render only the rows on screen: a
// 20,000-photo category would otherwise be 20,000 tiles at once.
// Pure — no Vue, Pinia or Tauri imports.
// ============================================================

export interface GridMetrics {
  /** Narrowest a tile may be: `minmax(minItemWidth, 1fr)` */
  minItemWidth: number
  /** Space between tiles, across and down */
  gap: number
  /** Tile height over width (3/4 for `aspect-4/3`) */
  aspect: number
}

export interface GridLayout {
  /** Tiles per row, as `repeat(auto-fill, …)` would fit them */
  columns: number
  /** Height of one row of tiles */
  rowHeight: number
  /** Row height plus the gap below it */
  rowStride: number
  rows: number
  /** Height of all rows, gaps between them included */
  height: number
}

/** The grid that `count` tiles make in a container `width` wide */
export function layoutGrid(
  count: number,
  width: number,
  { minItemWidth, gap, aspect }: GridMetrics,
): GridLayout {
  const columns = Math.max(1, Math.floor((width + gap) / (minItemWidth + gap)))
  const itemWidth = Math.max(0, (width - (columns - 1) * gap) / columns)
  const rowHeight = itemWidth * aspect
  const rowStride = rowHeight + gap
  const rows = Math.ceil(count / columns)
  const height = rows > 0 ? rows * rowStride - gap : 0
  return { columns, rowHeight, rowStride, rows, height }
}

/**
 * Tiles to render for a viewport `scrollTop`..`scrollTop + viewport`, whole
 * rows widened by `overscanRows` above and below: `start` inclusive, `end`
 * exclusive, and `top`, where the first rendered row sits
 */
export function visibleTiles(
  layout: GridLayout,
  count: number,
  scrollTop: number,
  viewport: number,
  overscanRows: number,
): { start: number; end: number; top: number } {
  const { columns, rowStride, rows } = layout
  if (rows === 0 || rowStride <= 0) return { start: 0, end: 0, top: 0 }
  // A viewport past the end (the content just shrank) shows the last rows
  const top = Math.min(Math.max(0, scrollTop), Math.max(0, layout.height - viewport))
  const firstRow = Math.max(0, Math.floor(top / rowStride) - overscanRows)
  const lastRow = Math.min(rows, Math.ceil((top + viewport) / rowStride) + overscanRows)
  return {
    start: firstRow * columns,
    end: Math.min(count, lastRow * columns),
    top: firstRow * rowStride,
  }
}
