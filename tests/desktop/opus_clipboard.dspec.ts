// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { test, expect } from '@playwright/test'
import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { launchApp, ipc } from './harness'

for (const slowOutput of [false, true]) test(`R86: clipboard paste precedes immediate Enter ${slowOutput ? 'during slow terminal output' : 'at an idle prompt'}`, async () => {
  const a = await launchApp({ env: { FAKE_CLI_PTY_BYTES_LOG: 'ordered.log' } })
  try {
    const p = a.page
    await p.getByTestId('nav-settings').click(); await p.getByTestId('run-setup').click()
    const terminal = p.getByTestId('term-claude')
    await expect(terminal).toContainText('>')
    if (slowOutput) {
      await ipc(p, 'pty_paste', { panel: 'claude', command: '1..100 | ForEach-Object { Write-Output "OUTPUT_ACTIVE"; Start-Sleep -Milliseconds 50 }' })
      await terminal.click(); await p.keyboard.press('Enter')
      await expect(terminal).toContainText('OUTPUT_ACTIVE')
    }
    await terminal.click()
    // Force asynchronous clipboard delivery even on a fast local IPC connection.
    // Enter is sent immediately; only the production shared input queue may hold it.
    const cdp = await p.context().newCDPSession(p)
    cdp.on('Fetch.requestPaused', (e: any) => {
      const release = () => void cdp.send('Fetch.continueRequest', { requestId: e.requestId }).catch(() => {})
      if (e.request.method === 'OPTIONS') release()
      else setTimeout(release, 200)
    })
    await cdp.send('Fetch.enable', { patterns: [{ urlPattern: 'http://ipc.localhost/pty_paste*', requestStage: 'Request' }] })
    const log = join(a.dataDir, 'ordered.log')
    const before = existsSync(log) ? readFileSync(log, 'utf8') : ''
    const started = Date.now()
    await terminal.locator('textarea').evaluate(el => {
      const data = new DataTransfer(); data.setData('text/plain', 'login-code')
      el.dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }))
    })
    await p.keyboard.press('Enter')
    await expect.poll(() => existsSync(log) ? readFileSync(log, 'utf8').slice(before.length) : '', { timeout: 1500 }).toBe('login-code\r')
    console.log(`clipboard paste and Enter recorded after ${Date.now() - started}ms`)
    await cdp.detach()
  } finally { await a.stop() }
})
