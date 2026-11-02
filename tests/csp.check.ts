// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Configuration guidance: https://v2.tauri.app/security/csp/
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
const config = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8'))
const csp = config.app.security.csp
assert.ok(csp, 'production CSP must be non-null')
assert.ok(!csp.includes('*') && !csp.includes("'unsafe-eval'"))
for (const directive of ["default-src 'self'", "script-src 'self'", "img-src 'self' data: blob:", "font-src 'self' data:", "connect-src 'self' ipc: http://ipc.localhost"]) assert.ok(csp.includes(directive), directive)
assert.deepEqual(JSON.parse(readFileSync('src-tauri/capabilities/default.json', 'utf8')).windows, ['main'])
console.log('csp check passed')
