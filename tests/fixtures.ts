// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Shared fixtures: every test fails on a console error or page error unless it opts out (the translation-crash test).
import { test as base, expect, type Page } from '@playwright/test'

export const test = base.extend<{ consoleErrors: string[]; allowConsoleErrors: boolean }>({
  allowConsoleErrors: [false, { option: true }],
  consoleErrors: [async ({ page, allowConsoleErrors }, use) => {
    const errors: string[] = []
    page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()) })
    page.on('pageerror', (e) => errors.push(`pageerror: ${e.message}`))
    await use(errors)
    if (!allowConsoleErrors) expect(errors, 'console errors').toEqual([])
  }, { auto: true }],
})
export { expect }

/** Speed of the canned run in tests (×20: a round takes about half a second). */
export const FAST = 'speed=20'

/** Skip the first-launch wizard: setup already finished. */
export async function setupDone(page: Page, extra: Record<string, unknown> = {}) {
  await page.addInitScript((x) => {
    if (!sessionStorage.getItem('seeded')) {
      localStorage.setItem('symvoulio.setup', JSON.stringify({ setupDone: true, ...x }))
      sessionStorage.setItem('seeded', '1')
    }
  }, extra)
}

export const term = (page: Page, p: 'claude' | 'codex') => page.getByTestId(`term-${p}`).locator('.xterm-rows')

/** No horizontal scroll of the page or of the content column. */
export async function noHorizontalScroll(page: Page, label = '') {
  const r = await page.evaluate(() => {
    const m = document.querySelector('[data-testid="main"]') as HTMLElement
    return { doc: document.documentElement.scrollWidth - document.documentElement.clientWidth, body: document.body.scrollWidth - document.body.clientWidth, main: m ? m.scrollWidth - m.clientWidth : 0 }
  })
  expect(r, `horizontal overflow ${label}`).toEqual({ doc: 0, body: 0, main: 0 })
}
