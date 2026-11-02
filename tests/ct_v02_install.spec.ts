// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// CT v0.2 install, DOD.2 (browser, canned): S1 Install buttons and the pasted text.
// Testids the feature must expose (the card names none; see report): s1-install-<p>, s1-install-hint-<p>, s1-install-msg-<p>.
import type { Page } from '@playwright/test'
import { FAST, expect, setupDone, term, test } from './fixtures'

const CLAUDE_WIN = 'irm https://claude.ai/install.ps1 | iex'
const CODEX_WIN = 'powershell -ExecutionPolicy ByPass -c "irm https://chatgpt.com/codex/install.ps1 | iex"'
const HINT = 'Press Enter in the terminal to install.'
const conn = (claude: boolean, codex: boolean) => ({
  conn: {
    claude: { installed: claude, version: claude ? '2.1.314' : '', connected: false, method: null },
    codex: { installed: codex, version: codex ? '0.9.0' : '', connected: false, method: null },
  },
})
/** True when the payload contains a real carriage return or line feed. */
const hasLineBreak = (v: string) => /[\r\n]/.test(v)
/** Physical xterm rows -> logical lines: a row exactly `cols` wide continues into the next (soft wrap); a shorter row ends the line. */
const logicalLines = (rows: string[], cols: number) => rows.reduce<string[]>((acc, r, i) => { if (i > 0 && rows[i - 1].length >= cols) acc[acc.length - 1] += r; else acc.push(r); return acc }, [])
const flat = (s: string) => s.replace(/\s+/g, '')

async function open(page: Page, claude: boolean, codex: boolean) {
  await setupDone(page, conn(claude, codex))
  await page.goto(`/setup/system?${FAST}`)
  await expect(page.getByTestId('s1-card-claude')).toBeVisible()
}

test.describe('V02 install: S1 buttons', () => {
  test('both missing: both cards have Install', async ({ page }) => {
    await open(page, false, false)
    await expect(page.getByTestId('s1-install-claude')).toBeVisible()
    await expect(page.getByTestId('s1-install-codex')).toBeVisible()
    await expect(page.getByTestId('s1-install-claude')).toHaveText(/Install/)
  })
  test('only Claude missing: only its card has Install; Check again and Continue stay', async ({ page }) => {
    await open(page, false, true)
    await expect(page.getByTestId('s1-install-claude')).toBeVisible()
    await expect(page.getByTestId('s1-install-codex')).toHaveCount(0)
    await expect(page.getByTestId('s1-installed-codex')).toHaveText('Yes')
    await expect(page.getByTestId('s1-check')).toBeVisible()
    await expect(page.getByTestId('s1-continue')).toBeEnabled()
  })
  test('only Codex missing: only its card has Install', async ({ page }) => {
    await open(page, true, false)
    await expect(page.getByTestId('s1-install-codex')).toBeVisible()
    await expect(page.getByTestId('s1-install-claude')).toHaveCount(0)
  })
  test('neither missing: no Install button anywhere', async ({ page }) => {
    await open(page, true, true)
    // Positive anchor so the absence is not vacuous: the page rendered the installed cards.
    await expect(page.getByTestId('s1-installed-claude')).toHaveText('Yes')
    await expect(page.getByTestId('s1-install-claude')).toHaveCount(0)
    await expect(page.getByTestId('s1-install-codex')).toHaveCount(0)
    // Discriminating control: the same page, Claude flipped to missing in the saved state, must now show Install
    // (an absent feature would pass the absence checks above vacuously).
    await page.evaluate(() => {
      const k = 'symvoulio.setup'; const v = JSON.parse(localStorage.getItem(k) || '{}')
      v.conn.claude.installed = false; localStorage.setItem(k, JSON.stringify(v))
    })
    await page.reload()
    await expect(page.getByTestId('s1-install-claude')).toBeVisible()
    await expect(page.getByTestId('s1-install-codex')).toHaveCount(0)
  })
})

test.describe('V02 install: paste, never Enter', () => {
  test('Install pastes the official command into that provider panel only, shows the hint, and waiting does not run it', async ({ page }) => {
    await open(page, false, false)
    await page.getByTestId('s1-install-claude').click()
    await expect(term(page, 'claude')).toContainText('claude.ai/install.ps1')
    await expect.poll(async () => flat(await term(page, 'claude').innerText())).toContain(flat(CLAUDE_WIN))
    await expect(page.getByTestId('s1-install-hint-claude')).toHaveText(HINT)
    await expect.poll(async () => flat(await term(page, 'codex').innerText())).not.toContain('install.ps1')
    await page.waitForTimeout(1500)
    // Not submitted: the card is still not installed and the Install button is still there.
    await expect(page.getByTestId('s1-installed-claude')).toHaveText('No')
    await expect(page.getByTestId('s1-install-claude')).toBeVisible()
    // Only the user's own Enter submits: the line count grows after it.
    const lines = async () => (await term(page, 'claude').innerText()).split('\n').filter((l) => l.trim()).length
    const before = await lines()
    await page.getByTestId('term-claude').click()
    await page.keyboard.press('Enter')
    await expect.poll(lines).toBeGreaterThan(before)
  })
  test('Codex Install pastes the quoted Windows command unchanged into the Codex panel', async ({ page }) => {
    await open(page, true, false)
    await page.getByTestId('s1-install-codex').click()
    await expect.poll(async () => flat(await term(page, 'codex').innerText())).toContain(flat(CODEX_WIN))
    await expect(page.getByTestId('s1-install-hint-codex')).toHaveText(HINT)
    await expect.poll(async () => flat(await term(page, 'claude').innerText())).not.toContain('chatgpt.com')
  })
  test('negative control: product refuses clipboard containing CR or LF', async ({ page }) => {
    const { pasteDecision } = await import('../src/lib/terminalInput')
    await open(page, false, true)
    await page.getByTestId('s1-install-claude').click()
    await expect(term(page, 'claude')).toContainText('claude.ai/install.ps1')
    for (const bad of [`${CLAUDE_WIN}\r`, `${CLAUDE_WIN}\n`]) expect(pasteDecision(bad)).toBe('refuse')
    expect(pasteDecision(CLAUDE_WIN)).toBe('paste')
  })
  test('pasted payload is one logical line without CR/LF (xterm rows joined where wrapped, production-safe)', async ({ page }) => {
    await open(page, false, true)
    await page.getByTestId('s1-install-claude').click()
    await expect(page.getByTestId('s1-install-hint-claude')).toBeVisible()
    await expect.poll(async () => {
      const rows = await term(page, 'claude').locator('> div').evaluateAll((els) => els.map((e) => (e.textContent ?? '').replace(/ /g, ' ').replace(/\s+$/, '')))
      const cols = Math.max(...rows.map((r) => r.length), 1)
      const lines = logicalLines(rows, cols).filter((l) => l.includes('claude.ai/install'))
      return lines.length === 1 && lines[0].includes(CLAUDE_WIN) && !hasLineBreak(lines[0])
    }, 'the command is a single logical line').toBe(true)
  })
  test('repeated click cannot duplicate the pending input', async ({ page }) => {
    await open(page, false, true)
    await page.getByTestId('s1-install-claude').click()
    await expect(page.getByTestId('s1-install-hint-claude')).toBeVisible()
    await page.getByTestId('s1-install-claude').click({ force: true })
    await page.getByTestId('s1-install-claude').click({ force: true })
    await page.waitForTimeout(500)
    await expect.poll(async () => flat(await term(page, 'claude').innerText()).split(flat(CLAUDE_WIN)).length - 1, 'command typed exactly once').toBe(1)
  })
  test('pending unsubmitted login input: Install refuses with a clear message and keeps the input', async ({ page }) => {
    await open(page, false, true)
    await page.getByTestId('s1-continue').click()
    await page.getByTestId('paste-login').click()
    await expect(term(page, 'claude')).toContainText('claude auth login')
    await page.getByTestId('back').click()
    await expect(page.getByTestId('s1-card-claude')).toBeVisible()
    await page.getByTestId('s1-install-claude').click()
    await expect(page.getByTestId('s1-install-msg-claude')).toBeVisible()
    await expect.poll(async () => flat(await term(page, 'claude').innerText())).toContain('claudeauthlogin')
    expect(flat(await term(page, 'claude').innerText())).not.toContain('install.ps1')
  })
})
