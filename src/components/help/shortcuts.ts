// ============================================================
// Shortcut reference shown by ShortcutHelp (keep in sync with useKeyboard)
// Order matters: groups flow into two balanced columns
// ============================================================

import { MOD_KEY } from '@/utils/platform'

export interface Shortcut {
  /** Alternative key chords; each chord is a list of keys pressed together */
  keys: string[][]
  label: string
}

export interface ShortcutGroup {
  title: string
  items: Shortcut[]
}

export const SHORTCUT_GROUPS: ShortcutGroup[] = [
  {
    title: '筛选',
    items: [
      { keys: [['F']], label: '标记精选 / 取消' },
      { keys: [['X'], ['Delete']], label: '标记删除 / 取消' },
      { keys: [['→'], ['Space']], label: '下一张' },
      { keys: [['←'], ['A']], label: '上一张' },
      { keys: [[MOD_KEY, 'Z']], label: '撤销' },
      { keys: [['L']], label: '标记后停在当前照片 / 自动前进' },
    ],
  },
  {
    title: '连拍组',
    items: [
      { keys: [[']']], label: '下一组第一张' },
      { keys: [['[']], label: '本组第一张 / 上一组' },
      { keys: [['Shift', 'F']], label: '精选这张，组内其余未标记的标删除' },
      { keys: [['Shift', 'X']], label: '组内未标记的全部标删除' },
      { keys: [['N']], label: '组内并排对比 / 退出' },
    ],
  },
  {
    title: '其他',
    items: [
      { keys: [[MOD_KEY, 'Enter']], label: '打开归档' },
      { keys: [['Esc']], label: '退出对比 / 组内对比 / 关闭面板' },
      { keys: [['?']], label: '显示本帮助' },
    ],
  },
  {
    title: '查看',
    items: [
      { keys: [['Z'], ['双击']], label: '在光标处切换 100% / 适应' },
      { keys: [['1']], label: '放大到 100%' },
      { keys: [['0']], label: '适应窗口' },
      { keys: [['+'], ['-']], label: '放大 / 缩小' },
      { keys: [['双指捏合'], ['滚轮']], label: '以光标为中心缩放' },
      { keys: [['双指滑动'], ['拖拽']], label: '放大后移动画面' },
      { keys: [['R']], label: '顺时针旋转 90°' },
      { keys: [['C']], label: '进入 / 退出对比' },
      { keys: [['I']], label: '显示 / 隐藏 EXIF' },
    ],
  },
]
