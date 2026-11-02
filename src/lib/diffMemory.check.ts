// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Failing before fix: AssertionError [ERR_ASSERTION]: R18 word diff must not allocate a quadratic matrix
/// <reference types="node" />
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { diffWords } from './diff'
assert(!readFileSync('src/lib/diff.ts', 'utf8').includes('const L: number[][]'), 'R18 word diff must not allocate a quadratic matrix')
const start = performance.now()
const parts = diffWords('old '.repeat(50000), 'new '.repeat(50000))
assert(performance.now() - start < 2000, 'R18 50,000 words under 2 seconds')
assert.deepEqual(parts.map(p => p.kind), ['del', 'add'])
console.log('R18 bounded word diff passed')

// Both block alignment and below-the-old-limit word alignment have bounded fallbacks.
const { diffDocs } = await import('./diff')
const changedBlocks = Array.from({ length: 20_000 }, (_, n) => ({ kind: 'p' as const, text: `old${n}` }))
const started = performance.now()
const blockResult = diffDocs(changedBlocks, changedBlocks.map((b, n) => ({ ...b, text: `new${n}` })))
assert.equal(blockResult.length, 20_000)
const wordResult = diffWords(Array.from({ length: 10_000 }, (_, n) => `a${n}`).join(' '), Array.from({ length: 10_000 }, (_, n) => `b${n}`).join(' '))
assert.equal(wordResult.length, 2)
assert(performance.now() - started < 1000)
console.log('R18 bounded block and word work passed')
