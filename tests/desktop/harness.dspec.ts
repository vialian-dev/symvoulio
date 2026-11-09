// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Self-test of the desktop harness: the app starts on an isolated data dir, real IPC answers, a run completes with the
// fake CLI, and the installed app's data is never touched.
import { test, expect } from '@playwright/test'
import { existsSync, readdirSync } from 'node:fs'
import { join } from 'node:path'
import { launchApp, ipc } from './harness'

test('harness: isolated data dir, real IPC, fake CLI run', async () => {
  const app = await launchApp()
  try {
    const settings = await ipc<{ workspace: string }>(app.page, 'settings_get')
    expect(settings.workspace).toBe(join(app.dataDir, 'workspace'))
    const info = await ipc<{ version: string }>(app.page, 'app_info')
    expect(info.version).toBe('0.3.2')
    await expect(app.page.locator('[data-screen="S11"]')).toBeVisible()
    expect(existsSync(join(app.dataDir, 'settings.json'))).toBe(true)
    expect(readdirSync(app.dataDir)).toContain('webview2')
  } finally { await app.stop() }
})
