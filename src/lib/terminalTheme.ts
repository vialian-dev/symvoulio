// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
export const PALETTE = {
  dark: { bg: '#050d22', fg: '#c4d6f5', cursor: '#6b9bff', sel: '#15306a', accent: '#6b9bff', ok: '#4fd1a5', err: '#ff6b70', ink: '#c4d6f5', gold: '#e2bd5b', key: '#9fb6dc', str: '#4fd1a5', num: '#8fb4ff', punct: '#6f84a8' },
  light: { bg: '#f7faff', fg: '#1b2d52', cursor: '#1f5fe0', sel: '#cfe0ff', accent: '#1f5fe0', ok: '#157a5a', err: '#c8343b', ink: '#1b2d52', gold: '#8a6510', key: '#2b4a7f', str: '#157a5a', num: '#1f5fe0', punct: '#526583' },
}
export type Pal = (typeof PALETTE)['dark']
export function xtermTheme(P: Pal) {
  // the xterm scrollbar in the family colours (thin by CSS in index.css).
  return { background: P.bg, foreground: P.fg, cursor: P.cursor, selectionBackground: P.sel,
    black: P.ink, red: P.err, green: P.ok, yellow: P.gold, blue: P.accent, magenta: P.key, cyan: P.str, white: P.ink,
    brightBlack: P.punct, brightRed: P.err, brightGreen: P.ok, brightYellow: P.gold, brightBlue: P.accent, brightMagenta: P.key, brightCyan: P.str, brightWhite: P.ink,
    scrollbarSliderBackground: P.punct + '66', scrollbarSliderHoverBackground: P.punct + '99', scrollbarSliderActiveBackground: P.accent + 'aa' }
}
