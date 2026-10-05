// ============================================================
// Sift - Survey View Rules
// N lays out the current burst side by side (up to four photos), all
// zoomed to the same relative point. The focus is the current photo:
// marks act on it and the arrow keys move it within the burst.
// No Vue, Pinia or Tauri imports.
// ============================================================

import { groupRange, type BurstGroups } from './grouping'
import type { PaneGrid, Size, ZoomState } from './zoom'

/** Most photos shown at once; a longer burst slides through them */
export const SURVEY_MAX = 4

/** Two photos side by side, three or four in a 2×2 grid */
export function surveyGrid(count: number): PaneGrid {
  if (count <= 1) return { cols: 1, rows: 1 }
  if (count === 2) return { cols: 2, rows: 1 }
  return { cols: 2, rows: 2 }
}

/** How many photos the survey of the burst holding `index` shows */
export function surveyCount(groups: BurstGroups, index: number): number {
  if (groups.groupOf[index] === undefined) return 1
  const { start, end } = groupRange(groups, index)
  return Math.min(end - start, SURVEY_MAX)
}

/**
 * Photos in view, `start` to `end` (exclusive), within the burst `range`.
 * The window stays put while the focus moves inside it and slides only as
 * far as needed to keep the focus in view, so panes don't all change at
 * every step. Without a usable previous window it starts at the focus.
 */
export function surveyWindow(
  prevStart: number | null,
  range: { start: number; end: number },
  focus: number,
): { start: number; end: number } {
  const size = Math.min(range.end - range.start, SURVEY_MAX)
  const lastStart = range.end - size
  const isPrevUsable = prevStart !== null && prevStart >= range.start && prevStart <= lastStart
  let start = isPrevUsable ? prevStart : focus
  if (focus < start) start = focus
  if (focus >= start + size) start = focus - size + 1
  start = Math.max(range.start, Math.min(start, lastStart))
  return { start, end: start + size }
}

/** The focus after an arrow key: within its burst, stopping at either end */
export function stepWithinGroup(groups: BurstGroups, index: number, delta: number): number {
  const { start, end } = groupRange(groups, index)
  return Math.max(start, Math.min(index + delta, end - 1))
}

/**
 * Rendered width and translation of a survey pane's photo. The zoom state is
 * the focus photo's (offsets in its fit-scale px); a photo fitted to another
 * size gets the offset scaled to its own fit, so every pane shows the same
 * relative point.
 */
export function surveyPaneTransform(
  fit: Size,
  focusFit: Size,
  state: ZoomState,
): { width: number; x: number; y: number } {
  const scaleX = focusFit.width ? fit.width / focusFit.width : 1
  const scaleY = focusFit.height ? fit.height / focusFit.height : 1
  return {
    width: fit.width * state.zoom,
    x: state.offsetX * scaleX * state.zoom,
    y: state.offsetY * scaleY * state.zoom,
  }
}
