// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { test, expect } from '@playwright/test'
import { launchApp, ipc } from './harness'

test('exited shell can be replaced and a stalled panel offers explicit recovery', async () => {
  const a = await launchApp()
  try {
    const p = a.page
    await p.getByTestId('nav-settings').click(); await p.getByTestId('run-setup').click()
    const terminal = p.getByTestId('term-claude')
    await expect(terminal).toContainText('>')
    await ipc(p, 'pty_paste', { panel: 'claude', command: 'exit' })
    await terminal.click(); await p.keyboard.press('Enter')
    await expect(p.getByTestId('reopen-claude')).toBeVisible()
    // Reattachment itself must replace the exited child, without pty_close.
    await p.reload()
    await expect(terminal).toContainText('>')
    await ipc(p, 'pty_paste', { panel: 'claude', command: 'function global:prompt { "NO_HOOK> " }' })
    await terminal.click(); await p.keyboard.press('Enter')
    await expect(p.getByTestId('reopen-claude')).toBeVisible({ timeout: 20000 })
    await p.getByTestId('reopen-claude').click()
    await expect(p.getByTestId('reopen-claude')).toHaveCount(0)
    await ipc(p, 'pty_paste', { panel: 'claude', command: "Write-Output ('RECOV' + 'ERED')" })
    await terminal.click(); await p.keyboard.press('Enter')
    await expect(terminal).toContainText('RECOVERED')
  } finally { await a.stop() }
})
