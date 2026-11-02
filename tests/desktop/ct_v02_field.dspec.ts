// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// CT field Highs, desktop. WRITTEN, NOT RUN in this session (the station's desktop is reserved for another run); run later in the full regression.



import { test, expect } from '@playwright/test'
import { existsSync, readdirSync } from 'node:fs'
import { execFileSync } from 'node:child_process'
import { launchApp, relaunch, ipc, BASE } from './harness'
import type { App } from './harness'

test.use({ actionTimeout: 15_000 })

async function normalRelaunch(a: App) {
  // WM_CLOSE flushes WebView2 storage before the shared harness reopens the profile.
  expect(execFileSync('powershell.exe', ['-NoProfile', '-Command', `(Get-Process -Id ${a.proc.pid}).CloseMainWindow()`], { encoding: 'utf8' }).trim()).toBe('True')
  await expect.poll(() => a.proc.exitCode, { timeout: 15_000 }).not.toBeNull()
  return relaunch(a)
}

test('bundled-sample Export either writes real files that exist, or the UI does not claim success; Open folder likewise', async () => {
  const a = await launchApp()
  try {
    const p = a.page
    await p.getByTestId('open-sample').click()
    await p.getByTestId('export').click()
    await p.waitForFunction(() => '__TAURI_INTERNALS__' in window)
    const before = existsSync(a.workspace) ? readdirSync(a.workspace, { recursive: true }).length : 0
    if (await p.getByTestId('do-export').isEnabled()) await p.getByTestId('do-export').click()
    const result = p.locator('[data-testid=export-result], [data-testid=sample-export-unavailable]')
    const shown = await result.innerText()
    const claimed = /exported|written/i.test(shown) && !/unsupported|not available|no files|prototype/i.test(shown)
    if (claimed) {
      const path = (await p.getByTestId('run-path').innerText()).trim()
      expect(existsSync(path), `UI says «${shown}» but ${path} does not exist`).toBe(true)
      expect(readdirSync(path, { recursive: true }).length, `UI says «${shown}» but ${path} is empty`).toBeGreaterThan(0)
      expect(readdirSync(a.workspace, { recursive: true }).length).toBeGreaterThan(before)
      // Open folder: the backend call must return without an error message for that existing path
      await p.getByTestId('open-folder').click()
      await expect(p.getByTestId('export-result')).not.toContainText(/error|does not exist|outside/i)
    } else {
      if (await p.getByTestId('do-export').isDisabled()) {
        await expect(p.getByTestId('sample-export-unavailable')).toBeVisible()
        await expect(p.getByTestId('open-folder')).toBeDisabled()
      }
      if (await p.getByTestId('open-folder').isEnabled()) await p.getByTestId('open-folder').click()
      await expect(result).not.toContainText(/opens in explorer/i)
    }
  } finally { await a.stop() }
})

async function expectWorkingPanels(a: App) {
  const p = a.page
  for (const provider of ['claude', 'codex']) {
    const id = `term-${provider}`
    await p.goto(`${BASE}/setup/${provider}`)
    await p.waitForFunction(() => '__TAURI_INTERNALS__' in window)
    await expect(p.getByTestId(id)).toBeVisible()
    await expect(p.getByTestId(id).locator('.xterm-rows')).not.toContainText(/Shell is unavailable/i, { timeout: 8_000 })
    await p.getByTestId(id).click(); await p.keyboard.type('echo ok')
    await expect(p.getByTestId(id).locator('.xterm-rows')).toContainText('echo ok')
    await expect(p.getByTestId(id).locator('.xterm-rows')).not.toContainText(/Shell is unavailable/i)
  }
}
test('saved shell "" with PowerShell present gives working panels, after the sample round-trip and after a relaunch', async () => {
  let a = await launchApp()
  try {
    // Persist the empty preference exactly as the field machine had it.
    await ipc(a.page, 'settings_set', { settings: { ...(await ipc<any>(a.page, 'settings_get')), shell: { claude: '', codex: '' } } })
    a = await normalRelaunch(a)
    await expectWorkingPanels(a)
    // sample round-trip: open the bundled sample, go to About, come back to the panels
    await a.page.getByTestId('nav-reviews').click()
    await a.page.getByTestId('open-sample').click()
    await a.page.getByTestId('nav-about').click()
    await expectWorkingPanels(a)
    a = await normalRelaunch(a)
    await expectWorkingPanels(a)
  } finally { await a.stop() }
})

test('a review that failed at start (oversized text) is still listed after a restart', async () => {
  let a = await launchApp()
  const NAME = 'CT failed at start'
  try {
    const p = a.page
    await p.getByTestId('new-review').click()
    await p.getByTestId('s6-name').fill(NAME)
    await p.getByTestId('s6-objective').fill('Decide whether the plan is sound.')
    await p.getByTestId('add-criterion').click()
    await p.getByTestId('criterion-0').fill('Totals must reconcile.')
    await p.getByTestId('approve-criteria').click()
    // over MAX_TEXT_BYTES (8 MB): the backend rejects the staged file before any run exists
    await p.getByTestId('primary-input').setInputFiles({ name: 'huge.md', mimeType: 'application/octet-stream', buffer: Buffer.from('x'.repeat(8 * 1024 * 1024 + 1024)) })
    await p.getByTestId('start').click()
    await expect(p.getByTestId('main').getByText('Failed', { exact: true })).toBeVisible({ timeout: 60_000 })
    await p.getByTestId('nav-reviews').click()
    await expect(p.getByText(NAME)).toBeVisible()
    a = await normalRelaunch(a)
    await a.page.getByTestId('nav-reviews').click()
    await expect(a.page.getByText(NAME), 'the failed review vanished after restart').toBeVisible()
  } finally { await a.stop() }
})
