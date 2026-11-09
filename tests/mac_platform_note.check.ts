// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// MAC-03/MAC-15: S13 distinguishes notarized Mac releases from self-built ad hoc apps.
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
assert.match(mac.why, /release download.*release page/)
assert.match(mac.why, /signed with an Apple Developer ID and notarized by Apple/)
assert.match(mac.why, /opens normally/)
assert.match(mac.why, /downloaded from the internet: click Open/)
assert.doesNotMatch(mac.why, /not notarized/)
assert.doesNotMatch(mac.title, /blocked/)
assert.match(mac.steps, /build.*yourself.*installers\/macos\/build\.sh.*without a Developer ID/)
assert.match(mac.steps, /signed ad hoc and not notarized/)
assert.match(mac.steps, /For that build/)
assert.match(mac.source, /Apple.*Open a Mac app from an unknown developer/)
assert.doesNotMatch(JSON.stringify(mac), /SmartScreen|Windows/)

const screen = await readFile(new URL('../src/screens/S13About.tsx', import.meta.url), 'utf8')
assert.match(screen, /data-testid="macos-note"/)
assert.match(screen, /firstRunNote\(info\?\.os\) === 'macos'/)
console.log('mac platform note check passed')
