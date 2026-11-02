// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Row R21 (shell settings are cosmetic). Pre-fix commit fda6004, fix commit f7d6d55. Old-API red variant.
// The saved shell chosen in Settings must cross the IPC boundary when a panel opens a PTY. At fda6004 `ptyOpen` has no
// shell parameter (a sixth argument is silently dropped), so the `pty_open` payload carries no `shell`. Uses only
// `desktopBackend.ptyOpen` with the Tauri IPC mocked, as the HEAD guard tests/shell.check.ts does.
// Run from the repo root: npx jiti tests/regression_variants/R21_shell_crosses_ipc.check.ts
import assert from 'node:assert/strict'
let args: Record<string, unknown> = {}
Object.defineProperty(globalThis, 'window', { configurable: true, value: { __TAURI_INTERNALS__: {
  transformCallback: () => 1,
  invoke: async (_command: string, input: Record<string, unknown>) => { args = input; return { shell: 'powershell', reattached: false } },
} } })
const { desktopBackend } = await import('../../src/services/backend')
const open = desktopBackend.ptyOpen as unknown as (...a: unknown[]) => Promise<unknown>
await open('codex', undefined, 80, 24, () => {}, 'powershell')
assert.equal(args.shell, 'powershell', 'the saved shell never crosses IPC: pty_open payload has no shell')
console.log('R21 shell crosses IPC: passed')
