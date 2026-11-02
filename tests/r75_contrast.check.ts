// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import assert from 'node:assert/strict'
import { PALETTE, xtermTheme } from '../src/lib/terminalTheme'
import { readFileSync } from 'node:fs'
const luminance = (hex: string) => {
  const c = hex.slice(1).match(/../g)!.map(v => parseInt(v, 16) / 255).map(v => v <= .04045 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4)
  return c[0] * .2126 + c[1] * .7152 + c[2] * .0722
}
const bg = luminance(PALETTE.light.bg)
for (const [key, value] of Object.entries(xtermTheme(PALETTE.light))) {
  if (/^(background|cursor|selection|scrollbar)/.test(key)) continue
  const fg = luminance(value)
  assert.ok((Math.max(bg, fg) + .05) / (Math.min(bg, fg) + .05) >= 4.5, `${key} must meet AA`)
}
const css = readFileSync('src/index.css', 'utf8')
const dim = css.match(/:root:not\(\.dark\) \[data-testid\^="term-"\] \.xterm-rows \.xterm-dim\s*\{\s*color:\s*(#[0-9a-f]{6}) !important/)
assert.ok(dim, 'light ANSI dim must override xterm translucent foreground')
assert.ok((bg + .05) / (luminance(dim[1]) + .05) >= 4.5, 'rendered dim text must meet AA')
assert.match(readFileSync('src/frame/Terminal.tsx', 'utf8'), /minimumContrastRatio: 9/, 'truecolor contrast floor')
console.log('Terminal normal, bright and dim contrast checks passed')
