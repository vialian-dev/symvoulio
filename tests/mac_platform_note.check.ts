// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// MAC-03/MAC-15: on a Mac, S13 explains the Gatekeeper first launch (Open Anyway), not Windows SmartScreen.
// Run: npx jiti tests/mac_platform_note.check.ts
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { firstRunNote } from '../src/lib/platformNote'

assert.equal(firstRunNote('macos'), 'macos')
for (const os of ['windows', 'linux', 'browser', undefined]) assert.equal(firstRunNote(os), 'windows', String(os))

const en = JSON.parse(await readFile(new URL('../src/i18n/en.json', import.meta.url), 'utf8'))
const mac = en.s13.mac
for (const key of ['title', 'why', 'steps', 'source']) assert.ok(typeof mac[key] === 'string' && mac[key].length > 0, key)
assert.match(mac.steps, /Privacy & Security/)
assert.match(mac.steps, /Open Anyway/)
assert.match(mac.why, /not notarized/)
assert.doesNotMatch(JSON.stringify(mac), /SmartScreen|Windows/)

const screen = await readFile(new URL('../src/screens/S13About.tsx', import.meta.url), 'utf8')
assert.match(screen, /data-testid="macos-note"/)
assert.match(screen, /firstRunNote\(info\?\.os\) === 'macos'/)
console.log('mac platform note check passed')
