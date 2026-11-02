// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// CT v0.2 R70 (jiti): the PowerShell prompt hook in src-tauri/src/pty.rs must report the CURRENT command's status.
// Runs the exact hook text under real PowerShell 7: native exit 7, then a successful cmdlet. Markers must be 7 then 0.
// Run: npx jiti tests/ct_v02_r66_70.check.ts
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { spawnSync } from 'node:child_process'

const src = readFileSync(new URL('../src-tauri/src/pty.rs', import.meta.url), 'utf8')
const m = src.match(/r#"(\$global:SymvoulioOriginalPrompt[\s\S]*?)"#/)
assert.ok(m, 'prompt hook literal found in pty.rs')
const hook = m![1]
const script = `${hook}; $env:SYMVOULIO_PROMPT_TOKEN='TOK'; cmd /c exit 7; prompt | Out-Null; Write-Output hi | Out-Null; prompt | Out-Null; Write-Output hi | Out-Null; $global:LASTEXITCODE = 0; cmd /c exit 0; prompt | Out-Null`
const r = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-Command', script], { encoding: 'utf8' })
assert.equal(r.status, 0, `pwsh ran: ${r.stderr}`)
const codes = [...r.stdout.matchAll(/\x1b\]133;D;TOK;(-?\d+)\x07/g)].map((x) => Number(x[1]))
console.log('markers', JSON.stringify(codes))
assert.deepEqual(codes, [7, 0, 0], 'native exit 7, then a successful cmdlet must report 0 (not the stale $LASTEXITCODE), then exit 0 stays 0')
console.log('R70 check passed')
