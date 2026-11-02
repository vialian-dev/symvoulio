// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// CT v0.2 install, DOD.3 / DOD.4 / DOD.5 / DOD.6 (desktop, fake CLI). WRITTEN, NOT RUNNABLE on 14f04c0:
//  1. No debug app / fake-cli binaries are built in this worktree (target/ absent); building is out of scope for a tests-only step.
//  2. tests/desktop/harness.ts cannot start with an absent CLI or give the fake CLI an "installer": no env such as
//     FAKE_CLI_ABSENT / FAKE_CLI_INSTALL_* and no PTY byte log exist. This spec names the fixture env the coder must add.
//  PS5.1 / PS7 / macOS zsh variants: NOT MEASURED.
import { test, expect } from '@playwright/test'
import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import type { Page } from '@playwright/test'
import { launchApp, ipc } from './harness'

/** Reach S1 the way a user does: Settings -> Run setup again (the harness opens /reviews with setup done). */
async function toS1(p: Page) {
  await p.getByTestId('nav-settings').click()
  await p.getByTestId('run-setup').click()
  await expect(p.getByTestId('s1-card-claude')).toBeVisible()
}

test.use({ actionTimeout: 15_000 })
const SECRET = 'sk-ant-FAKESECRET123456'

async function missingApp(extra: Record<string, string> = {}) {
  return launchApp({ scenario: 'ok', env: { FAKE_CLI_ABSENT: 'claude', FAKE_CLI_INSTALL_TARGET_VERSION: '9.8.7', FAKE_CLI_PTY_BYTES_LOG: 'pty_bytes.log', ...extra } })
}
const bytes = (dir: string) => (existsSync(join(dir, 'pty_bytes.log')) ? readFileSync(join(dir, 'pty_bytes.log'), 'latin1') : '')

test('DOD.3: Install pastes via pty_paste with no CR/LF; installer does not start before the user Enter; Enter starts it exactly once', async () => {
  const app = await missingApp()
  try {
    const p = app.page
    await toS1(p)
    await p.getByTestId('s1-install-claude').click()
    await expect(p.getByTestId('s1-install-hint-claude')).toHaveText('Press Enter in the terminal to install.')
    await p.waitForTimeout(2000)
    expect(bytes(app.dataDir)).toContain('claude.ai/install.ps1')
    expect(bytes(app.dataDir)).not.toMatch(/[\r\n]/)
    expect(existsSync(join(app.dataDir, 'fake_installer_started'))).toBe(false)
    await p.getByTestId('term-claude').click()
    await p.keyboard.press('Enter')
    await expect.poll(() => { const f = join(app.dataDir, 'fake_installer_started'); return existsSync(f) ? readFileSync(f, 'utf8').trim().split('\n').length : 0 }).toBe(1)
  } finally { await app.stop() }
})

test('DOD.4: after the fake install Check again flips only that card to Installed 9.8.7 and removes Install, no restart', async () => {
  const app = await missingApp()
  try {
    const p = app.page
    await toS1(p)
    await p.getByTestId('s1-install-claude').click()
    await p.getByTestId('term-claude').click()
    await p.keyboard.press('Enter')
    await p.getByTestId('s1-check').click()
    await expect(p.getByTestId('s1-installed-claude')).toHaveText('Yes')
    await expect(p.getByTestId('s1-version-claude')).toHaveText('9.8.7')
    await expect(p.getByTestId('s1-install-claude')).toHaveCount(0)
  } finally { await app.stop() }
})

test('DOD.5: nonzero exit with a secret last line -> named error, real code, redacted line, still missing, retry possible', async () => {
  const app = await missingApp({ FAKE_CLI_INSTALL_EXIT: '7', FAKE_CLI_INSTALL_LAST_LINE: `auth failed token=${SECRET}` })
  try {
    const p = app.page
    await toS1(p)
    await p.getByTestId('s1-install-claude').click()
    await p.getByTestId('term-claude').click()
    await p.keyboard.press('Enter')
    const msg = p.getByTestId('s1-install-msg-claude')
    await expect(msg).toContainText('Claude Code install failed')
    await expect(msg).toContainText('7')
    // R66 boundary (COO 01.11 13:46Z): the terminal shows raw output, like any terminal; everything outside it stays clean
    await expect(msg).not.toContainText(SECRET)
    expect(await p.evaluate(() => { const b = document.body.cloneNode(true) as HTMLElement; b.querySelectorAll('[data-testid^="term-"]').forEach((e) => e.remove()); return b.innerText })).not.toContain(SECRET)
    await expect(p.getByTestId('s1-installed-claude')).toHaveText('No')
    await expect(p.getByTestId('s1-install-claude')).toBeEnabled()
    await expect(p.getByTestId('term-claude').locator('.xterm-rows')).toContainText(SECRET)
    await p.getByTestId('s1-install-claude').click()
    await expect(p.getByTestId('s1-install-hint-claude')).toBeVisible()
    await p.getByTestId('term-claude').click()
    await p.keyboard.press('Enter')
    await expect.poll(() => readFileSync(join(app.dataDir, 'fake_installer_started'), 'utf8').trim().split('\n').length).toBe(2)
    await expect(msg).toContainText('Claude Code install failed')
  } finally { await app.stop() }
})

test('DOD.5b: exit 0 without a detectable CLI -> "Installation not detected"', async () => {
  const app = await missingApp({ FAKE_CLI_INSTALL_EXIT: '0', FAKE_CLI_INSTALL_NOOP: '1' })
  try {
    const p = app.page
    await toS1(p)
    await p.getByTestId('s1-install-claude').click()
    await p.getByTestId('term-claude').click()
    await p.keyboard.press('Enter')
    await p.getByTestId('s1-check').click()
    await expect(p.getByTestId('s1-install-msg-claude')).toContainText('Installation not detected')
    await expect(p.getByTestId('s1-installed-claude')).toHaveText('No')
  } finally { await app.stop() }
})

test('IPC routing only; DOD.6 post-install isolation canary NOT MEASURED', async () => {
  const app = await missingApp()
  try {
    const p = app.page
    await toS1(p)
    await p.getByTestId('s1-install-claude').click()
    const log = String(await ipc(p, 'pty_debug_log', {}).catch(() => ''))
    expect(log).toMatch(/pty_paste/)
    expect(log).not.toMatch(/pty_write/)
  } finally { await app.stop() }
})
