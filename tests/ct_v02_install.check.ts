// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// CT v0.2 install, DOD.1 (jiti): command selection per provider x OS/shell x state x Node.
// Run: npx jiti tests/ct_v02_install.check.ts
//
// CONTRACT UNDER TEST (the card names no module or signature; this is the minimal seam, see report):
//   src/lib/installCommands.ts exports
//     installPlan(i: { provider: 'claude'|'codex'; os: 'windows'|'macos'|'linux';
//                      shell: string /* 'pwsh' | 'powershell' | 'zsh' | 'bash' | path or other */;
//                      installed?: boolean; node?: 'present'|'missing'|'old'; nativeAvailable?: boolean })
//       => { ok: true; command: string; method: 'native'|'npm' } | { ok: false; error: string; link?: string }
// Commands are the verbatim rows of .hardening/v02/OFFICIAL_INSTALL.md.
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'

const CLAUDE_WIN = 'irm https://claude.ai/install.ps1 | iex'
const CLAUDE_SH = 'curl -fsSL https://claude.ai/install.sh | bash'
const CODEX_WIN = 'powershell -ExecutionPolicy ByPass -c "irm https://chatgpt.com/codex/install.ps1 | iex"'
const CODEX_SH = 'curl -fsSL https://chatgpt.com/codex/install.sh | sh'
const NPM = { claude: 'npm install -g @anthropic-ai/claude-code', codex: 'npm install -g @openai/codex' }

type Plan = { ok: true; command: string; method: string } | { ok: false; error: string; link?: string }
let installPlan: (i: Record<string, unknown>) => Plan
try {
  ({ installPlan } = await import('../src/lib/installCommands') as { installPlan: typeof installPlan })
} catch (e) {
  console.error(`FAIL DOD.1 src/lib/installCommands.ts is absent: ${(e as Error).message.split('\n')[0]}`)
  process.exit(1)
}

const cases: [string, string, string, string][] = [
  ['claude', 'windows', 'pwsh', CLAUDE_WIN], ['claude', 'windows', 'powershell', CLAUDE_WIN],
  ['claude', 'macos', 'zsh', CLAUDE_SH], ['claude', 'linux', 'bash', CLAUDE_SH],
  ['codex', 'windows', 'pwsh', CODEX_WIN], ['codex', 'windows', 'powershell', CODEX_WIN],
  ['codex', 'macos', 'zsh', CODEX_SH], ['codex', 'linux', 'bash', CODEX_SH],
]
for (const [provider, os, shell, want] of cases) {
  // Native is preferred whatever Node/npm look like; never an npm line.
  for (const node of ['present', 'missing', 'old', undefined]) {
    const r = installPlan({ provider, os, shell, installed: false, node })
    assert.ok(r.ok, `${provider}/${os}/${shell}/node=${node} must plan an install`)
    if (!r.ok) continue
    assert.equal(r.command, want, `${provider}/${os}/${shell}/node=${node}`)
    assert.equal(r.method, 'native', `native preferred for ${provider}/${os}/${shell}/node=${node}`)
    assert.ok(!/npm/.test(r.command), 'no npm when native exists')
    assert.ok(!/[\x00-\x1f\x7f]/.test(r.command), `control byte in ${provider}/${os}/${shell}`)
  }
}
// PS7 keeps the quoted Codex Windows command unchanged: no pwsh-only variant.
const ps7 = installPlan({ provider: 'codex', os: 'windows', shell: 'pwsh', installed: false })
assert.ok(ps7.ok && ps7.command === CODEX_WIN && ps7.command.startsWith('powershell ') && !/pwsh/.test(ps7.command), 'PS7 Codex command unchanged')
// A shell given as a full path is recognised the way shellCommands.ts does.
const pathShell = installPlan({ provider: 'claude', os: 'windows', shell: 'C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe', installed: false })
assert.ok(pathShell.ok && pathShell.command === CLAUDE_WIN, 'powershell.exe full path')

// Already installed: no install plan.
for (const provider of ['claude', 'codex']) {
  const r = installPlan({ provider, os: 'windows', shell: 'pwsh', installed: true })
  assert.equal(r.ok, false, `${provider} installed -> no install plan`)
}

// Unsupported / incompatible shell: the named error, never a guess.
for (const [os, shell] of [['windows', 'cmd'], ['windows', 'C:\\Windows\\System32\\cmd.exe'], ['windows', 'zsh'], ['macos', 'pwsh'], ['macos', 'fish'], ['linux', 'powershell'], ['windows', '']]) {
  for (const provider of ['claude', 'codex']) {
    const r = installPlan({ provider, os, shell, installed: false, node: 'present' })
    assert.ok(!r.ok, `${provider}/${os}/${shell || '(empty)'} must be refused`)
    if (!r.ok) assert.match(r.error, /Unsupported install shell/, 'named error')
  }
}

// npm-only fallback (no native method): npm when Node is usable, otherwise the Node.js link; never an auto-install.
for (const provider of ['claude', 'codex'] as const) {
  const ok = installPlan({ provider, os: 'macos', shell: 'zsh', installed: false, node: 'present', nativeAvailable: false })
  assert.ok(ok.ok && ok.command === NPM[provider] && ok.method === 'npm', `${provider} npm fallback`)
  for (const node of ['missing', 'old']) {
    const r = installPlan({ provider, os: 'macos', shell: 'zsh', installed: false, node, nativeAvailable: false })
    assert.ok(!r.ok, `${provider} node=${node} without native -> no command`)
    if (!r.ok) {
      assert.match(r.error, /Node\.js is required/)
      assert.equal(r.link, 'https://nodejs.org/en/download')
    }
  }
}
// The planner is pure: it must not execute Node/npm to decide.
const src = await readFile(new URL('../src/lib/installCommands.ts', import.meta.url), 'utf8')
assert.ok(!/child_process|execSync|spawn\(/.test(src), 'planner executes nothing')
console.log('ct_v02_install check passed')
