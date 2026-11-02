// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { test, expect } from '@playwright/test'
import { mkdirSync, writeFileSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { launchApp, ipc } from './harness'

test('provider commands resolve freshly after an external install without replacing the panel', async () => {
  const a = await launchApp({ env: { SYMVOULIO_FAKE_CLI: '', FAKE_CLI_INSTALL_NO_PATH: '1', FAKE_CLI_PTY_BYTES_LOG: 'paths.log' } })
  try {
    const p = a.page
    await p.getByTestId('nav-settings').click(); await p.getByTestId('run-setup').click()
    const terminal = p.getByTestId('term-claude')
    await expect(terminal).toContainText('>')
    const bin = join(a.dataDir, 'fake_bin'); mkdirSync(bin)
    for (const name of ['claude', 'codex']) writeFileSync(join(bin, `${name}.cmd`), '@echo off\r\necho RESOLVED_%*\r\n')
    for (const command of ['claude auth login --console', 'claude auth status', 'codex login status', 'codex login --with-api-key']) {
      await ipc(p, 'pty_paste', { panel: 'claude', command })
      expect(readFileSync(join(a.dataDir, 'paths.log'), 'utf8').toLowerCase()).toContain(`& '${bin}\\${command.split(' ')[0]}.cmd' ${command.split(' ').slice(1).join(' ')}`.toLowerCase())
      await terminal.click(); await p.keyboard.press('Enter')
      await expect.poll(async () => (await terminal.innerText()).replace(/\s+/g, '')).toContain(`RESOLVED_${command.split(' ').slice(1).join('')}`)
    }
  } finally { await a.stop() }
})
