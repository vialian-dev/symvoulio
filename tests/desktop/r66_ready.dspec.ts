// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { test, expect } from '@playwright/test'
import { launchApp } from './harness'

test('focus reports leave a clean prompt installable while history recall blocks it', async () => {
 const a = await launchApp({port:9891, scenario:'ok', env:{FAKE_CLI_ABSENT:'claude'}})
 try {
  const p=a.page
  await p.getByTestId('nav-settings').click(); await p.getByTestId('run-setup').click()
  await p.getByTestId('term-claude').click()
  await p.getByTestId('s1-install-claude').click()
  await expect(p.getByTestId('s1-install-hint-claude')).toBeVisible()
  await p.getByTestId('term-claude').click(); await p.keyboard.press('Control+u'); await p.keyboard.press('ArrowUp')
  await p.getByTestId('s1-install-claude').click()
  await expect(p.getByTestId('s1-install-msg-claude')).toContainText('pending input or is busy')
 } finally { await a.stop() }
})
