// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import assert from 'node:assert/strict'
import { aliasesKey, loadAliases, persistAliases, pruneAliases } from './runAliases'
const data = new Map<string, string>()
Object.defineProperty(globalThis, 'localStorage', { value: { getItem: (k: string) => data.get(k), setItem: (k: string, v: string) => data.set(k, v) } })
const ids = new Map(Array.from({ length: 250 }, (_, n) => [`alias-${n}`, `run-${n}`]))
persistAliases('C:/workspace-a', ids)
assert.equal(loadAliases('C:/workspace-a').length, 200)
assert.equal(loadAliases('C:/workspace-a')[0][0], 'alias-50')
assert.deepEqual(loadAliases('C:/workspace-b'), [])
pruneAliases(ids, new Set(['run-249']), new Set(['run-248']))
persistAliases('C:/workspace-a', ids)
assert.deepEqual(loadAliases('C:/workspace-a'), [['alias-248', 'run-248'], ['alias-249', 'run-249']])
assert.notEqual(aliasesKey('C:/workspace-a'), aliasesKey('C:/workspace-b'))
console.log('Alias persistence: workspace isolation, last 200 bound, removed-run pruning and in-flight binding protection passed')
