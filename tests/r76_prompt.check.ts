// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { spawnSync } from 'node:child_process'
const hook = readFileSync('src-tauri/src/pty.rs', 'utf8').match(/r#"(\$global:SymvoulioOriginalPrompt[\s\S]*?)"#/)![1]
for (const shell of ['powershell', 'pwsh']) {
 const script = `${hook}; $env:SYMVOULIO_PROMPT_TOKEN='TOK'; cmd /c exit 7; prompt | Out-Null; cmd /c exit 7; prompt | Out-Null; cmd /c exit 0; Write-Error failure; prompt | Out-Null; Write-Output ok | Out-Null; prompt | Out-Null`
 const r = spawnSync(shell, ['-NoProfile', '-NonInteractive', '-Command', script], {encoding:'utf8'})
 assert.equal(r.status, 0, r.stderr)
 const codes = [...r.stdout.matchAll(/\x1b\]133;D;TOK;(-?\d*)\x07/g)].map(x => x[1])
 console.log(shell, JSON.stringify(codes))
 assert.equal(codes[0], '7'); assert.ok(['7',''].includes(codes[1])); assert.equal(codes[2], ''); assert.equal(codes[3], '0')
}
