// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import assert from 'node:assert/strict'
import { readFileSync, existsSync } from 'node:fs'
assert.ok(existsSync('THIRD_PARTY_NOTICES.md'), 'third-party notices must ship')
const notices = readFileSync('THIRD_PARTY_NOTICES.md', 'utf8')
const pkg = JSON.parse(readFileSync('package.json', 'utf8'))
for (const name of Object.keys(pkg.dependencies).filter(n => n.startsWith('@fontsource/'))) {
  assert.ok(notices.includes(name), name)
  const path = `public/licenses/${name.split('/')[1]}.txt`
  assert.ok(existsSync(path), path)
  assert.ok(readFileSync(path, 'utf8').includes(readFileSync(`node_modules/${name}/LICENSE`, 'utf8')))
}
assert.ok(JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8')).bundle.resources.includes('../THIRD_PARTY_NOTICES.md'))
assert.ok(notices.includes('tauri |') && notices.includes('react |') && notices.includes('@xterm/xterm |'))
console.log('notices check passed')
