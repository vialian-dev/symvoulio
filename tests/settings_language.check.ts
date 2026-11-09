// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Run: npx jiti tests/settings_language.check.ts
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'

const en = JSON.parse(await readFile(new URL('../src/i18n/en.json', import.meta.url), 'utf8'))
assert.match(en.s12.languageHint, /English only/)
assert.doesNotMatch(en.s12.languageHint, /\bv?\d+\.\d+/i)
console.log('settings language check passed')
