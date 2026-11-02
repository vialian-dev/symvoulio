// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { test, expect } from '@playwright/test'
import { readFileSync } from 'node:fs'
import { launchApp, ipc } from './harness'

test('connection detection succeeds with pending terminal input and login refusal names the action', async () => {
  const a = await launchApp({ env: { FAKE_CLI_PTY_BYTES_LOG: 'connection-input.log' } })
  try {
    const p = a.page
    await p.getByTestId('nav-settings').click(); await p.getByTestId('run-setup').click()
    await p.getByTestId('s1-continue').click()
    await p.getByTestId('paste-login').click()
    await expect.poll(() => ipc<string>(p, 'pty_debug_log')).not.toBe('')
    await expect(ipc(p, 'pty_paste', { panel: 'claude', command: 'claude auth login' })).rejects.toThrow('before pasting a login or status command')
    const before = readFileSync(a.fakeLog, 'utf8').length
    await p.getByTestId('check-connection').click()
    await expect(p.getByTestId('check-connection')).toBeEnabled()
    await expect.poll(() => readFileSync(a.fakeLog, 'utf8').length).toBeGreaterThan(before)
    await expect(p.getByTestId('connected')).toBeVisible()
    await expect(p.getByTestId('connection-result')).toHaveCount(0)
  } finally { await a.stop() }
})
