// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import assert from 'node:assert/strict'
import { apiKeyLogin } from '../src/lib/shellCommands'
for (const shell of ['pwsh', 'powershell', 'C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe']) assert.equal(apiKeyLogin('codex', shell), '$env:OPENAI_API_KEY | codex login --with-api-key')
for (const shell of ['zsh', '/bin/zsh', 'bash']) assert.equal(apiKeyLogin('codex', shell), 'printenv OPENAI_API_KEY | codex login --with-api-key')
assert.equal(apiKeyLogin('claude', 'zsh'), 'claude auth login --console')
let args: Record<string, unknown> = {}
Object.defineProperty(globalThis, 'window', { configurable: true, value: { __TAURI_INTERNALS__: {
  transformCallback: () => 1,
  invoke: async (_command: string, input: Record<string, unknown>) => { args = input; return { shell: 'powershell', reattached: false } },
} } })
const { desktopBackend } = await import('../src/services/backend')
const actual = await desktopBackend.ptyOpen('codex', undefined, 80, 24, () => {}, 'powershell')
assert.equal(args.shell, 'powershell', 'saved shell crosses IPC')
assert.equal(actual.shell, 'powershell', 'actual shell comes from PtyInfo')
console.log('shell check passed')
