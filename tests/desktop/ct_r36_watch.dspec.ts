// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// CARD R36w (cruel tester): a file change that is not part of the frontend must not reload the running app.
// Measured: with `touch crates/symvoulio-core/src/lib.rs` every 2 s, VAL-R36 failed because the dev server reloaded the page
// about every 2.1 s; each reload re-mounted S6 and wiped the form. Rule tested: while the S6 form is filled, rewriting (same
// bytes) a Rust source, the Tauri entry point, a scratch file under .hardening/ and one under test-results/ causes no
// main-frame navigation and leaves the typed text in place. Frontend files are deliberately NOT touched (they may hot-update).
import { test, expect } from '@playwright/test'
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { launchApp } from './harness'

test.use({ actionTimeout: 15_000 })

const MARKER = 'R36w-marker-7741'
const REWRITTEN = ['crates/symvoulio-core/src/lib.rs', 'src-tauri/src/main.rs'].map((p) => resolve(p))
const SCRATCH = [resolve('.hardening', 'ct_r36_watch.scratch.txt'), resolve('test-results', 'ct_r36_watch.scratch.txt')]
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))

test('R36w: touching Rust, .hardening and test-results files does not reload the page or wipe the S6 form', async () => {
  const originals = new Map(REWRITTEN.map((f) => [f, readFileSync(f)]))
  const createdDirs: string[] = []
  const app = await launchApp()
  try {
    const p = app.page
    await p.getByTestId('new-review').click()
    await p.getByTestId('s6-name').fill(MARKER)
    await expect(p.getByTestId('s6-name')).toHaveValue(MARKER)

    const navigations: string[] = []
    const t0 = Date.now()
    p.on('framenavigated', (f) => { if (f === p.mainFrame()) navigations.push(`${Date.now() - t0}ms ${f.url()}`) })

    for (const dir of [resolve('.hardening'), resolve('test-results')]) {
      if (!existsSync(dir)) { mkdirSync(dir, { recursive: true }); createdDirs.push(dir) }
    }
    for (let i = 0; i < 8; i++) {
      for (const f of REWRITTEN) writeFileSync(f, readFileSync(f)) // same bytes, new mtime
      for (const f of SCRATCH) writeFileSync(f, `touch ${i} ${Date.now()}\n`)
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
    for (const f of SCRATCH) rmSync(f, { force: true })
    for (const d of createdDirs) rmSync(d, { recursive: true, force: true })
    await app.stop()
  }
})
