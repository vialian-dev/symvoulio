// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// CARD R59b (cruel tester): a design-system edit must still hot-update the dev app. src/index.css imports
// packages/design-system/src/styles/theme.css, so appending a rule that changes one of its custom properties must change the
// computed value on <html> within 10 s, WITHOUT a main-frame navigation. The file is restored byte for byte.
import { test, expect } from '@playwright/test'
import { appendFileSync, readFileSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { launchApp } from './harness'

test.use({ actionTimeout: 15_000 })

const THEME = resolve('packages', 'design-system', 'src', 'styles', 'theme.css')
const VAR = '--color-ice-50'
const NEXT = '#123456'
const read = (page: import('@playwright/test').Page) =>
  page.evaluate((v) => getComputedStyle(document.documentElement).getPropertyValue(v).trim(), VAR)

test('R59b: appending a :root override to the design-system theme.css hot-updates the page without a navigation', async () => {
  const original = readFileSync(THEME)
  const eol = original.includes(Buffer.from('\r\n')) ? '\r\n' : '\n'
  const app = await launchApp()
  try {
    const p = app.page
    const before = await read(p)
    expect(before, `${VAR} is not defined on :root before the edit`).not.toBe('')
    expect(before.toLowerCase()).not.toBe(NEXT)
    await p.waitForTimeout(2000) // settle
    const navigations: string[] = []
    p.on('framenavigated', (f) => { if (f === p.mainFrame()) navigations.push(f.url()) })

    appendFileSync(THEME, `${eol}:root { ${VAR}: ${NEXT}; }${eol}`)

    await expect.poll(() => read(p), { timeout: 10_000, message: `${VAR} stayed «${before}»: the design-system edit was not picked up` }).toBe(NEXT)
    expect(navigations, 'the update reloaded the page instead of hot-updating').toHaveLength(0)
  } finally {
    writeFileSync(THEME, original)
    expect(readFileSync(THEME).equals(original), 'theme.css restored byte for byte').toBe(true)
    await app.stop()
  }
})
