// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { test, expect } from '@playwright/test'
import { launchApp, ipc } from './harness'
test('15 second guidance keeps install locked and attributes the later result', async () => {
  const app = await launchApp({ scenario: 'ok', env: { FAKE_CLI_ABSENT: 'claude', FAKE_CLI_INSTALL_DELAY_MS: '24000', FAKE_CLI_INSTALL_EXIT: '3', FAKE_CLI_INSTALL_LAST_LINE: 'download failed' } })
  try {
    const p = app.page
    await p.getByTestId('nav-settings').click()
    await p.getByTestId('run-setup').click()
    await p.getByTestId('s1-install-claude').click()
    await p.getByTestId('s1-install-hint-claude').waitFor()
    await p.getByTestId('term-claude').click()
    await p.keyboard.press('Enter')
    await expect(p.getByTestId('s1-install-msg-claude')).toHaveText('Still running — check manually when the installer finishes', { timeout: 22000 })
    await expect(ipc(p, 'pty_paste', { panel: 'claude', command: 'echo retry', guard: true })).rejects.toThrow('pending input or is busy')
    await expect(p.getByTestId('s1-install-msg-claude')).toContainText('Claude Code install failed', { timeout: 15000 })
    await expect(p.getByTestId('s1-install-msg-claude')).toContainText('3')
    await ipc(p, 'pty_paste', { panel: 'claude', command: 'echo after', guard: true })
  } finally { await app.stop() }
})
