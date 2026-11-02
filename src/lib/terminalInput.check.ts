// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
const assert = { equal(actual: string, expected: string, message = '') { if (actual !== expected) throw new Error(`${message}: ${actual} !== ${expected}`) } }
import { pasteDecision } from './terminalInput'
for (const text of ['one\ntwo', 'echo hi\r', '\t', '\0', '\x1b[200~x\x1b[201~', '\x7f', '\u0085']) {
  assert.equal(pasteDecision(text), 'refuse', `unsafe clipboard text accepted: ${JSON.stringify(text)}`)
}
for (const text of ['', 'echo hi', 'unicode \u00e9']) assert.equal(pasteDecision(text), 'paste')
console.log('terminalInput checks passed')
