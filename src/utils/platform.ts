export const IS_MAC = navigator.userAgent.includes('Mac')

/** Label for the primary modifier: ⌘ on macOS, Ctrl elsewhere */
export const MOD_KEY = IS_MAC ? '⌘' : 'Ctrl'
