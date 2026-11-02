// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Align revisions by block text, retaining deleted blocks and diffing replacement words.
import type { Block } from '../data/sample'

export type Part = { kind: 'same' | 'add' | 'del'; text: string }

/** Hirschberg LCS: two score rows, with index ranges instead of copied slices. */
function align(x: string[], y: string[], emit: (kind: Part['kind'], text: string) => void) {
  if (x.length * y.length > 1_000_000 || Math.max(x.length, y.length) > 4000) {
    x.forEach(text => emit('del', text)); y.forEach(text => emit('add', text)); return
  }
  function score(a: number, b: number, c: number, d: number, reverse: boolean) {
    let row = new Uint32Array(d - c + 1)
    for (let i = 0; i < b - a; i++) {
      const next = new Uint32Array(d - c + 1)
      for (let j = 0; j < d - c; j++) next[j + 1] = x[reverse ? b - 1 - i : a + i] === y[reverse ? d - 1 - j : c + j] ? row[j] + 1 : Math.max(row[j + 1], next[j])
      row = next
    }
    return row
  }
  function visit(a: number, b: number, c: number, d: number) {
    if (a === b) { for (let j = c; j < d; j++) emit('add', y[j]); return }
    if (c === d) { for (let i = a; i < b; i++) emit('del', x[i]); return }
    if (b - a === 1) {
      const match = y.indexOf(x[a], c)
      if (match < c || match >= d) { emit('del', x[a]); for (let j = c; j < d; j++) emit('add', y[j]) }
      else { for (let j = c; j < match; j++) emit('add', y[j]); emit('same', x[a]); for (let j = match + 1; j < d; j++) emit('add', y[j]) }
      return
    }
    const mid = (a + b) >>> 1
    let split = 0
    { const left = score(a, mid, c, d, false), right = score(mid, b, c, d, true)
      for (let j = 1; j <= d - c; j++) if (left[j] + right[d - c - j] > left[split] + right[d - c - split]) split = j
    }
    visit(a, mid, c, c + split); visit(mid, b, c + split, d)
  }
  let start = 0, endX = x.length, endY = y.length
  while (start < endX && start < endY && x[start] === y[start]) { emit('same', x[start]); start++ }
  while (endX > start && endY > start && x[endX - 1] === y[endY - 1]) { endX--; endY-- }
  visit(start, endX, start, endY)
  for (let i = endX; i < x.length; i++) emit('same', x[i])
}
export function diffWords(a: string, b: string): Part[] {
  if (a === b) return [{ kind: 'same', text: a }]
  if (Math.max(a.trim().split(/\s+/).length, b.trim().split(/\s+/).length) > 20000) return [{ kind: 'del', text: a }, { kind: 'add', text: b }]
  const out: Part[] = []
  align(a.split(/(\s+)/), b.split(/(\s+)/), (kind, text) => {
    const last = out.at(-1)
    if (last?.kind === kind) last.text += text
    else out.push({ kind, text })
  })
  return out
}

export interface BlockDiff { kind: Block['kind']; changed: boolean; parts: Part[] }
export function diffDocs(a: Block[], b: Block[]): BlockDiff[] {
  const rows: BlockDiff[] = []
  let i = 0, j = 0
  let removed: Block[] = [], added: Block[] = []
  const flush = () => {
    for (let k = 0; k < Math.max(removed.length, added.length); k++) {
      const old = removed[k], next = added[k]
      rows.push({ kind: (next ?? old).kind, changed: true, parts: old && next ? diffWords(old.text, next.text) : [{ kind: old ? 'del' : 'add', text: (old ?? next).text }] })
    }
    removed = []; added = []
  }
  align(a.map(block => block.text), b.map(block => block.text), (kind, text) => {
    if (kind === 'del') removed.push(a[i++])
    else if (kind === 'add') added.push(b[j++])
    else { flush(); rows.push({ kind: b[j].kind, changed: false, parts: [{ kind: 'same', text }] }); i++; j++ }
  })
  flush()
  return rows
}
