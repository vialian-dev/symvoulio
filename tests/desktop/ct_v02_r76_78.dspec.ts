// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// CT R76-R78, desktop. WRITTEN, NOT RUN in this session (the station's desktop is reserved for another run); run later in the full regression.
// R77: real OS-clipboard paste (Ctrl+V in the terminal) after "echo " appends exactly; clipboard paste into a busy process is delivered;
//      the S2 Paste command button still refuses while input is pending / busy and says so on screen; repeated S2 Paste = one command.
// R76: install failure reports the real native status (7) and never calls a missing status an «exit code».
import { test, expect } from '@playwright/test'
import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { execFileSync } from 'node:child_process'
import type { Page } from '@playwright/test'
import { launchApp, ipc, BASE } from './harness'

test.use({ actionTimeout: 15_000 })
const read = (dir: string, name: string) => (existsSync(join(dir, name)) ? readFileSync(join(dir, name), 'latin1') : '')
async function toS1(p: Page) {
  await p.getByTestId('nav-settings').click()
  await p.getByTestId('run-setup').click()
  await expect(p.getByTestId('s1-card-claude')).toBeVisible()
}
/** The user's clipboard, then Ctrl+V with the terminal focused. */
async function clipboardPaste(p: Page, text: string, pid: number) {
  await p.context().grantPermissions(['clipboard-read', 'clipboard-write'])
  await p.getByTestId('term-claude').click()
  const encoded = Buffer.from(text, 'utf16le').toString('base64')
  execFileSync('powershell.exe', ['-NoProfile', '-STA', '-Command', `$ErrorActionPreference = 'Stop'; Set-Clipboard -Value ([Text.Encoding]::Unicode.GetString([Convert]::FromBase64String('${encoded}')))`])
  expect(await p.evaluate(() => navigator.clipboard.readText())).toBe(text)
  execFileSync('powershell.exe', ['-NoProfile', '-STA', '-Command', `Add-Type -AssemblyName System.Windows.Forms; $shell = New-Object -ComObject WScript.Shell; if (-not $shell.AppActivate(${pid})) { throw 'owned app activation failed' }; Start-Sleep -Milliseconds 200; [System.Windows.Forms.SendKeys]::SendWait('^v')`])
}

test('R77: OS clipboard paste after typing «echo » appends exactly (no refusal text)', async () => {
  const a = await launchApp({ env: { FAKE_CLI_ABSENT: 'claude', FAKE_CLI_PTY_BYTES_LOG: 'input.log' } })
  try {
    const p = a.page
    await toS1(p)
    await p.getByTestId('term-claude').click()
    await p.keyboard.type('echo ')
    await expect.poll(() => read(a.dataDir, 'input.log')).toBe('echo ')
    await clipboardPaste(p, 'clipboard-argument', a.proc.pid!)
    await expect.poll(() => read(a.dataDir, 'input.log')).toBe('echo clipboard-argument')
    await expect(p.getByTestId('term-claude').locator('.xterm-rows')).not.toContainText(/pending input|busy/i)
  } finally { await a.stop() }
})

test('R77: OS clipboard paste into a busy (waiting) process is delivered', async () => {
  const a = await launchApp({ env: { FAKE_CLI_ABSENT: 'claude', FAKE_CLI_INSTALL_DELAY_MS: '12000', FAKE_CLI_PTY_BYTES_LOG: 'busy.log' } })
  try {
    const p = a.page
    await toS1(p)
    await p.getByTestId('s1-install-claude').click()
    await p.getByTestId('term-claude').click(); await p.keyboard.press('Enter')
    await expect.poll(() => existsSync(join(a.dataDir, 'fake_installer_started'))).toBe(true)
    await expect(ipc(p, 'pty_paste', { panel: 'claude', command: 'duplicate' })).rejects.toThrow('pending input or is busy')
    await p.getByTestId('s1-continue').click()
    await p.getByTestId('paste-login').click()
    await expect(p.getByRole('alert').filter({ hasText: /pending input|busy/i })).toBeVisible()
    await clipboardPaste(p, 'auth-code-1234', a.proc.pid!)
    await expect.poll(() => read(a.dataDir, 'busy.log')).toContain('auth-code-1234')
  } finally { await a.stop() }
})

test('R77: S2 Paste is refused while input is pending, the refusal is visible, and repeated S2 Paste leaves exactly one login command', async () => {
  const a = await launchApp({ env: { FAKE_CLI_ABSENT: 'claude', FAKE_CLI_PTY_BYTES_LOG: 'login.log' } })
  try {
    const p = a.page
    await p.goto(`${BASE}/setup/claude`)
    await p.waitForFunction(() => '__TAURI_INTERNALS__' in window)
    // pending input typed by the user: the button must refuse and say so
    await p.getByTestId('term-claude').click()
    await p.keyboard.type('half')
    await expect.poll(() => read(a.dataDir, 'login.log')).toBe('half')
    await p.getByTestId('paste-login').click()
    await expect(p.getByRole('alert').filter({ hasText: /pending input|busy/i })).toBeVisible()
    expect(read(a.dataDir, 'login.log')).toBe('half')
    // clear the line, paste once, then twice more: exactly one login command is typed
    await ipc(p, 'pty_write', { panel: 'claude', data: '\u0015' })
    await p.getByTestId('paste-login').click()
    await expect.poll(() => (read(a.dataDir, 'login.log').match(/claude auth login/g) ?? []).length).toBe(1)
    await p.getByTestId('paste-login').click(); await p.getByTestId('paste-login').click()
    await p.waitForTimeout(1500)
    expect((read(a.dataDir, 'login.log').match(/claude auth login/g) ?? []).length).toBe(1)
    await expect(p.getByRole('alert').filter({ hasText: /pending input|busy/i })).toBeVisible()
  } finally { await a.stop() }
})

test('R76: failed install shows the real native status 7 and never labels it «exit code»', async () => {
  const a = await launchApp({ scenario: 'ok', env: { FAKE_CLI_ABSENT: 'claude', FAKE_CLI_INSTALL_EXIT: '7', FAKE_CLI_INSTALL_LAST_LINE: 'installer failed', FAKE_CLI_PTY_BYTES_LOG: 'pty_bytes.log' } })
  try {
    const p = a.page
    await toS1(p)
    await p.getByTestId('s1-install-claude').click()
    await p.getByTestId('term-claude').click(); await p.keyboard.press('Enter')
    const msg = p.getByTestId('s1-install-msg-claude')
    await expect(msg).toContainText('Claude Code install failed')
    await expect(msg).toContainText('7')
    await expect(msg).not.toContainText(/exit code/i)
    // R87 harness-only: prove a fresh attempt replaces the old failure, then
    // wait for its own completion/failure. The previous 7 or empty text cannot pass.
    await p.getByTestId('s1-install-claude').click()
    await expect(msg).toHaveCount(0)
    await p.getByTestId('term-claude').click(); await p.keyboard.press('Enter')
    await expect.poll(() => read(a.dataDir, 'fake_installer_started').trim().split(/\r?\n/)).toEqual(['claude', 'claude'])
    await expect(msg).toContainText('Claude Code install failed')
    await expect(msg).toContainText(/status (7|unavailable)\)/)
    await expect(msg).not.toContainText(/exit code/i)
  } finally { await a.stop() }
})
