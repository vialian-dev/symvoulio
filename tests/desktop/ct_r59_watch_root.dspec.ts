// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// CARD R59 (cruel tester): the sibling of ct_r36_watch. A root document (README.md, CHANGELOG.md, KNOWN_LIMITATIONS.md,
// SECURITY.md) is not part of the frontend: rewriting it (same bytes) while the S6 form is filled must cause no main-frame
// navigation and must not wipe the typed text.
import { test, expect } from '@playwright/test'
import { readFileSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { launchApp } from './harness'

test.use({ actionTimeout: 15_000 })

const MARKER = 'R59-marker-3318'
const DOCS = ['README.md', 'CHANGELOG.md', 'KNOWN_LIMITATIONS.md', 'SECURITY.md'].map((p) => resolve(p))
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))

test('R59: rewriting README, CHANGELOG, KNOWN_LIMITATIONS and SECURITY does not reload the page or wipe the S6 form', async () => {
  const originals = new Map(DOCS.map((f) => [f, readFileSync(f)]))
  const app = await launchApp()
  try {
    const p = app.page
    await p.getByTestId('new-review').click()
    await p.getByTestId('s6-name').fill(MARKER)
    await expect(p.getByTestId('s6-name')).toHaveValue(MARKER)
    await sleep(2000) // settle: a navigation of the click itself is not the subject

    const navigations: string[] = []
    const t0 = Date.now()
    p.on('framenavigated', (f) => { if (f === p.mainFrame()) navigations.push(`${Date.now() - t0}ms ${f.url()}`) })

    for (let i = 0; i < 8; i++) {
      for (const f of DOCS) writeFileSync(f, readFileSync(f)) // same bytes, new mtime
      await sleep(1000)
    }
    await sleep(1500) // a reload triggered by the last touch

    expect(navigations, `the page navigated ${navigations.length} time(s) after the fill: ${JSON.stringify(navigations)}`).toHaveLength(0)
    await expect(p.getByTestId('s6-name'), 'the S6 name field lost the typed text').toHaveValue(MARKER)
  } finally {
    for (const [f, bytes] of originals) {
      writeFileSync(f, bytes)
      expect(readFileSync(f).equals(bytes), `${f} restored byte for byte`).toBe(true)
    }
    await app.stop()
  }
})
