// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Failing before fix: AssertionError [ERR_ASSERTION]: R17 deleted last paragraph must have a row
/// <reference types="node" />
import assert from 'node:assert/strict'
import { diffDocs } from './diff'
const blocks = (...texts: string[]) => texts.map(text => ({ kind: 'p' as const, text }))
assert(diffDocs(blocks('keep', 'last'), blocks('keep')).some(r => r.parts.some(p => p.kind === 'del' && p.text === 'last')), 'R17 deleted last paragraph must have a row')
assert(diffDocs(blocks('a', 'b'), []).every(r => r.parts.every(p => p.kind === 'del')))
assert.equal(diffDocs(blocks('a', 'b'), []).length, 2)
for (const next of [blocks('x', 'a', 'b'), blocks('b', 'a')]) {
 const rows = diffDocs(blocks('a', 'b'), next)
 assert.equal(rows.flatMap(r => r.parts.filter(p => p.kind !== 'add').map(p => p.text)).join(''), 'ab')
 assert.equal(rows.flatMap(r => r.parts.filter(p => p.kind !== 'del').map(p => p.text)).join(''), next.map(b => b.text).join(''))
}
console.log('R17 block alignment passed')
