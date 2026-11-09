// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { test, expect } from '@playwright/test'
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { launchApp, relaunch } from './harness'

test('install outside PATH refreshes the existing panel and survives relaunch', async () => {
  const dataDir = mkdtempSync(join(tmpdir(), 'symvoulio-e2e-'))
  const pathFile = join(dataDir, 'fake_user_path')
  writeFileSync(pathFile, '')
  let a = await launchApp({ dataDir, scenario: 'ok', env: { FAKE_CLI_ABSENT: 'claude', FAKE_CLI_INSTALL_NO_PATH: '1', SYMVOULIO_USER_PATH_FILE: pathFile } })
  try {
    const p = a.page
    await p.getByTestId('nav-settings').click()
    await p.getByTestId('run-setup').click()
    await p.getByTestId('s1-install-claude').click()
    await p.getByTestId('term-claude').click()
    await p.keyboard.press('Enter')
    await expect.poll(() => existsSync(join(dataDir, 'fake_bin'))).toBe(true)
    await expect(p.getByTestId('s1-installed-claude')).toHaveText('Yes')
    expect(readFileSync(pathFile, 'utf8')).toBe('')
    const check = async () => {
      await a.page.getByTestId('s1-continue').click()
      await a.page.getByTestId('paste-login').click()
      await a.page.getByTestId('term-claude').click()
      await a.page.keyboard.press('Enter')
      await expect.poll(async () => (await a.page.getByTestId('term-claude').locator('.xterm-rows').innerText()).replace(/\s+/g, '')).toContain('claude-fake9.8.7')
      expect(await a.page.getByTestId('term-claude').innerText()).not.toContain('command not found')
    }
    await check()
    a = await relaunch(a)
    await a.page.getByTestId('nav-settings').click()
    await a.page.getByTestId('run-setup').click()
    await check()
    expect(readFileSync(pathFile, 'utf8')).toBe('')
  } finally { await a.stop() }
})
