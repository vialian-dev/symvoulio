// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { test, expect } from '@playwright/test'
import { launchApp, ipc } from './harness'

test('repeating S2 Paste preserves a single pending login', async () => {
  const a = await launchApp({ scenario: 'ok', env: { FAKE_CLI_ABSENT: 'codex' } })
  try {
    const p = a.page
    await p.getByTestId('nav-settings').click()
    await p.getByTestId('run-setup').click()
    await p.getByTestId('s1-continue').click()
    await p.getByTestId('paste-login').click()
    await p.getByTestId('paste-login').click()
    await expect(p.getByText('The panel has pending input or is busy.', { exact: false })).toBeVisible()
    await expect.poll(async () => (await p.getByTestId('term-claude').locator('.xterm-rows').innerText()).replace(/\s+/g, '').split('claudeauthlogin').length - 1).toBe(1)
    await expect(ipc(p, 'pty_paste', { panel: 'claude', command: 'claude auth login' })).rejects.toThrow('pending input')
  } finally { await a.stop() }
})
