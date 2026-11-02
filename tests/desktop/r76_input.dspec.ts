// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { test, expect } from '@playwright/test'
import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { launchApp, relaunch, ipc } from './harness'
test('raw keyboard order survives startup and relaunch; clipboard appends while command buttons refuse', async () => {
 let a = await launchApp({ env: { FAKE_CLI_ABSENT: 'claude', FAKE_CLI_PTY_BYTES_LOG: 'input.log' } })
 try {
  for (let pass = 0; pass < 3; pass++) {
   if (pass) { a = await relaunch(a); await a.page.evaluate(() => localStorage.setItem('symvoulio.setup', JSON.stringify({setupDone:true}))); await a.page.goto('http://127.0.0.1:5193/reviews') }
   const p = a.page
   await p.getByTestId('nav-settings').click(); await p.getByTestId('run-setup').click()
   await p.getByTestId('term-claude').click()
   await p.keyboard.type('claude auth login')
   await expect.poll(() => existsSync(join(a.dataDir, 'input.log')) ? readFileSync(join(a.dataDir, 'input.log'), 'utf8') : '').toBe('claude auth login'.repeat(pass + 1))
   await expect(ipc(p, 'pty_paste', {panel:'claude', command:'claude auth login'})).rejects.toThrow('pending input or is busy')
  }
  await ipc(a.page, 'pty_write', {panel:'claude', data:'\u0015'})
  await ipc(a.page, 'pty_write', {panel:'claude', data:'echo '})
  await ipc(a.page, 'pty_paste', {panel:'claude', command:'clipboard-argument', clipboard:true})
  await expect(ipc(a.page, 'pty_paste', {panel:'claude', command:'duplicate'})).rejects.toThrow('pending input or is busy')
  await ipc(a.page, 'pty_write', {panel:'claude', data:'\r'})
 } finally { await a.stop() }
})

test('clipboard authentication response is accepted while a process is busy', async () => {
 const a = await launchApp({env:{FAKE_CLI_ABSENT:'claude', FAKE_CLI_INSTALL_DELAY_MS:'3000', FAKE_CLI_PTY_BYTES_LOG:'busy.log'}})
 try {
  const p = a.page
  await p.getByTestId('nav-settings').click(); await p.getByTestId('run-setup').click()
  await p.getByTestId('s1-install-claude').click()
  await p.getByTestId('term-claude').click(); await p.keyboard.press('Enter')
  await expect.poll(() => existsSync(join(a.dataDir, 'fake_installer_started'))).toBe(true)
  await expect(ipc(p,'pty_paste',{panel:'claude', command:'duplicate'})).rejects.toThrow('pending input or is busy')
  await ipc(p,'pty_paste',{panel:'claude', command:'auth-code', clipboard:true})
  expect(readFileSync(join(a.dataDir,'busy.log'),'utf8')).toContain('auth-code')
 } finally { await a.stop() }
})
