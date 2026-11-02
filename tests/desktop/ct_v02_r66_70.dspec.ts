// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// CT v0.2 R66-R69 (desktop, fake installer shell). Red on b2e624e. R70 lives in tests/ct_v02_r66_70.check.ts (real PowerShell hook).
// Fixture knobs (additive, in crates/symvoulio-core/src/bin/fake-cli.rs; defaults keep old behaviour):
//   FAKE_CLI_INSTALL_RAW=1          emit FAKE_CLI_INSTALL_LAST_LINE UNREDACTED (R66)
//   FAKE_CLI_INSTALL_DELAY_MS=<n>   installer still running after n ms (R67)
//   SYMVOULIO_USER_PATH_FILE=<file> stand-in for the user PATH: the installer writes its bin dir there; the fake shell finds
//                                   claude/codex only if ITS OWN process PATH lists that dir (R69). THE APP MUST read this file
//                                   (debug builds) wherever it reads the registry user PATH, and give panels the refreshed PATH.
import { test, expect, type Page } from '@playwright/test'
import { mkdtempSync, readdirSync, readFileSync, statSync, existsSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { launchApp } from './harness'

test.use({ actionTimeout: 15_000 })
const SECRET = 'sk-ant-api03-ZZSYNTHETICsecret0123456789abcdef'
const flat = (s: string) => s.replace(/\s+/g, '')

async function toS1(p: Page) {
  await p.getByTestId('nav-settings').click()
  await p.getByTestId('run-setup').click()
  await expect(p.getByTestId('s1-card-claude')).toBeVisible()
}
async function app(absent: string, extra: Record<string, string> = {}) {
  const dataDir = mkdtempSync(join(tmpdir(), 'symvoulio-e2e-'))
  return launchApp({ dataDir, scenario: 'ok', env: { FAKE_CLI_ABSENT: absent, FAKE_CLI_INSTALL_TARGET_VERSION: '9.8.7', SYMVOULIO_USER_PATH_FILE: join(dataDir, 'fake_user_path'), ...extra } })
}
function filesContaining(dir: string, needle: string): string[] {
  const hits: string[] = []
  const walk = (d: string) => {
    for (const n of readdirSync(d)) {
      const f = join(d, n)
      let s
      try { s = statSync(f) } catch { continue }
      if (s.isDirectory()) walk(f)
      else if (s.size < 64e6) { try { const b = readFileSync(f); if (b.includes(needle) || b.includes(Buffer.from(needle, 'utf16le'))) hits.push(f) } catch { /* locked */ } }
    }
  }
  walk(dir)
  return hits
}
const outsideTerminal = (p: Page) => p.evaluate(() => {
  const c = document.body.cloneNode(true) as HTMLElement
  c.querySelectorAll('[data-testid^="term-"]').forEach((e) => e.remove())
  return c.textContent ?? ''
})

test('R66: raw installer output may reach the terminal, but card, UI outside the terminal and every file under the app data dir stay clean of the secret', async () => {
  const a = await app('claude', { FAKE_CLI_INSTALL_EXIT: '7', FAKE_CLI_INSTALL_RAW: '1', FAKE_CLI_INSTALL_LAST_LINE: `auth failed token=${SECRET}` })
  try {
    const p = a.page
    await toS1(p)
    await p.getByTestId('s1-install-claude').click()
    await p.getByTestId('term-claude').click()
    await p.keyboard.press('Enter')
    const msg = p.getByTestId('s1-install-msg-claude')
    await expect(msg).toContainText('Claude Code install failed')
    await expect(msg).toContainText('7')
    expect(await msg.innerText(), 'error card').not.toContain(SECRET)
    expect(await outsideTerminal(p), 'UI outside the terminal').not.toContain(SECRET)
    await expect(p.getByTestId('term-claude').locator('.xterm-rows')).toContainText(SECRET)
    // Terminal may show it raw. A reload re-attaches the panel (replay backlog): no other surface may gain it.
    await p.reload()
    await expect(p.getByTestId('nav-settings')).toBeVisible()
    expect(await outsideTerminal(p), 'UI after reload').not.toContain(SECRET)
    expect(filesContaining(a.dataDir, SECRET), 'files in the app data dir holding the secret').toEqual([])
  } finally { await a.stop(true) }
})

test('R69: after a native install the same panel finds the CLI without an app restart (user PATH refreshed)', async () => {
  const a = await app('codex')
  try {
    const p = a.page
    await toS1(p)
    await p.getByTestId('s1-install-codex').click()
    await p.getByTestId('term-codex').click()
    await p.keyboard.press('Enter')
    await expect.poll(() => existsSync(join(a.dataDir, 'fake_user_path'))).toBe(true)
    await p.getByTestId('s1-check').click()
    await expect(p.getByTestId('s1-installed-codex')).toHaveText('Yes')
    // A fresh command in that panel, as the S2/S3 login would type it.
    await p.getByTestId('term-codex').click()
    await p.keyboard.type('codex --version')
    await p.keyboard.press('Enter')
    const text = () => p.getByTestId('term-codex').locator('.xterm-rows').innerText().then(flat)
    await expect.poll(async () => { const t = await text(); return t.includes('codex-fake9.8.7') || t.includes('commandnotfound') }, { timeout: 15_000 }).toBe(true)
    const t = await text()
    expect(t, 'panel must not say command not found').not.toContain('commandnotfound')
    expect(t).toContain('codex-fake9.8.7')
  } finally { await a.stop() }
})

test('R67: an install still running after 15 s is not shown as finished; a second Install is refused; the later real result is attributed to the install', async () => {
  test.setTimeout(120_000)
  const a = await app('claude', { FAKE_CLI_INSTALL_DELAY_MS: '24000', FAKE_CLI_INSTALL_EXIT: '3', FAKE_CLI_INSTALL_LAST_LINE: 'download failed' })
  try {
    const p = a.page
    await toS1(p)
    await p.getByTestId('s1-install-claude').click()
    await p.getByTestId('term-claude').click()
    await p.keyboard.press('Enter')
    await expect.poll(() => existsSync(join(a.dataDir, 'fake_installer_started'))).toBe(true)
    await p.waitForTimeout(17_000)
    await p.getByTestId('s1-install-claude').click({ force: true })
    await expect(p.getByTestId('s1-install-msg-claude'), 'second Install while the first still runs must be refused').toBeVisible()
    const t = flat(await p.getByTestId('term-claude').locator('.xterm-rows').innerText())
    expect(t.split('claude.ai/install').length - 1, 'literal typed once').toBe(1)
    await expect(p.getByTestId('s1-install-msg-claude')).toContainText('Claude Code install failed', { timeout: 30_000 })
    await expect(p.getByTestId('s1-install-msg-claude')).toContainText('3')
  } finally { await a.stop() }
})

test('R68: history recall (Up arrow at an empty prompt) blocks the Install paste', async () => {
  const a = await app('claude')
  try {
    const p = a.page
    await toS1(p)
    await p.getByTestId('term-claude').click()
    await p.keyboard.press('ArrowUp')
    await p.waitForTimeout(500)
    await p.getByTestId('s1-install-claude').click()
    await expect(p.getByTestId('s1-install-msg-claude'), 'unknown editor state must refuse the paste').toBeVisible()
    expect(flat(await p.getByTestId('term-claude').locator('.xterm-rows').innerText())).not.toContain('claude.ai/install')
  } finally { await a.stop() }
})
