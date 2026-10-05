import { describe, expect, it } from 'vitest'
import { PhotoStatus, type PhotoPair } from '@/types'
import { groupPairs } from './grouping'
import {
  stepWithinGroup,
  surveyCount,
  surveyGrid,
  surveyPaneTransform,
  surveyWindow,
} from './survey'

const SECOND = 1000

function shot(seconds: number[]): PhotoPair[] {
  return seconds.map((s, i) => ({
    id: `p${i}`,
    jpgPath: `/photos/IMG_${i}.JPG`,
    rawPath: null,
    rawFormat: null,
    status: PhotoStatus.Unprocessed,
    thumbnailPath: null,
    dominantColor: null,
    source: 'jpg',
    xmpPaths: [],
    captureTime: { localMs: s * SECOND, offsetMinutes: null },
  }))
}

// Bursts [0-1], [2] and [3-8]
const groups = groupPairs(shot([0, 1, 10, 20, 21, 22, 23, 24, 25]), 2 * SECOND)

describe('surveyGrid', () => {
  it('puts two photos side by side and three or four in a 2×2 grid', () => {
    expect(surveyGrid(1)).toEqual({ cols: 1, rows: 1 })
    expect(surveyGrid(2)).toEqual({ cols: 2, rows: 1 })
    expect(surveyGrid(3)).toEqual({ cols: 2, rows: 2 })
    expect(surveyGrid(4)).toEqual({ cols: 2, rows: 2 })
  })
})

describe('surveyCount', () => {
  it('shows the whole burst, at most four photos', () => {
    expect(surveyCount(groups, 1)).toBe(2)
    expect(surveyCount(groups, 2)).toBe(1)
    expect(surveyCount(groups, 5)).toBe(4)
  })
})

describe('surveyWindow', () => {
  const big = { start: 3, end: 9 } // six photos

  it('starts at the focus, kept inside the burst', () => {
    expect(surveyWindow(null, big, 3)).toEqual({ start: 3, end: 7 })
    expect(surveyWindow(null, big, 8)).toEqual({ start: 5, end: 9 })
  })

  it('stays put while the focus moves inside it', () => {
    expect(surveyWindow(3, big, 6)).toEqual({ start: 3, end: 7 })
  })

  it('slides just far enough to keep the focus in view', () => {
    expect(surveyWindow(3, big, 7)).toEqual({ start: 4, end: 8 })
    expect(surveyWindow(5, big, 4)).toEqual({ start: 4, end: 8 })
  })

  it('starts over when the focus moves to another burst', () => {
    expect(surveyWindow(5, { start: 0, end: 2 }, 1)).toEqual({ start: 0, end: 2 })
  })
})

describe('stepWithinGroup', () => {
  it('moves the focus inside its burst and stops at its ends', () => {
    expect(stepWithinGroup(groups, 3, 1)).toBe(4)
    expect(stepWithinGroup(groups, 8, 1)).toBe(8)
    expect(stepWithinGroup(groups, 3, -1)).toBe(3)
    expect(stepWithinGroup(groups, 1, -1)).toBe(0)
  })
})

describe('surveyPaneTransform', () => {
  const state = { zoom: 2, offsetX: 30, offsetY: -20 }

  it('renders a photo the size of the focus exactly like the focus', () => {
    const fit = { width: 600, height: 400 }
    expect(surveyPaneTransform(fit, fit, state)).toEqual({ width: 1200, x: 60, y: -40 })
  })

  it('aims a photo of another size at the same relative point', () => {
    const focus = { width: 600, height: 400 }
    const smaller = { width: 300, height: 400 }
    expect(surveyPaneTransform(smaller, focus, state)).toEqual({ width: 600, x: 30, y: -40 })
  })

  it('keeps the offset as is before the focus is measured', () => {
    const fit = { width: 600, height: 400 }
    expect(surveyPaneTransform(fit, { width: 0, height: 0 }, state)).toEqual({
      width: 1200,
      x: 60,
      y: -40,
    })
  })
})
