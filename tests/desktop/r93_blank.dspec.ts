// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { test, expect } from '@playwright/test'
import { existsSync, readFileSync } from 'node:fs'
import { launchApp } from './harness'

test('blank primary shows a preflight reason with zero provider calls', async () => {
  const app = await launchApp()
  try {
    for (const text of ['', ' \r\n\t\u2003', '\uFEFF \n']) {
      await app.page.getByTestId('nav-new').click()
      await app.page.getByTestId('s6-name').fill('Blank input')
      await app.page.getByTestId('s6-objective').fill('Check the facts.')
      await app.page.getByTestId('add-criterion').click()
      await app.page.getByTestId('criterion-0').fill('Facts have evidence.')
      await app.page.getByTestId('approve-criteria').click()
      await app.page.getByTestId('primary-input').setInputFiles({ name: 'blank.md', mimeType: 'text/markdown', buffer: Buffer.from(text) })
      await app.page.getByTestId('start').click()
      await expect(app.page.getByRole('alert')).toContainText('Primary document is empty or contains only whitespace')
      await expect(app.page.getByTestId('status-state')).toHaveText('Failed')
      await expect(app.page.getByTestId('status-strip')).not.toContainText('Round')
      // Concurrent version/auth probes can interleave log lines; review invocations use -p / exec.
      expect(readFileSync(app.fakeLog, 'utf8')).not.toMatch(/"(?:-p|exec)"/)
      expect(existsSync(`${app.fakeLog}.scenarios`)).toBe(false)
    }
  } finally { await app.stop() }
})
