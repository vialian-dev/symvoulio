// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// CARD P5b (cruel tester): VAL-R52ui. The fake reviewer reports a NEW issue as `resolved` in round 1 (scenario
// unknown_resolved). The run must not pause or report a malformed answer; the issue must be visible in the workspace
// issue list (S7) with «reported already resolved in round 1»; final/final_review.html must say the same (browser-free).
import { test, expect } from '@playwright/test'
import { existsSync, mkdtempSync, readdirSync, readFileSync, statSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { launchApp, scenariosRun } from './harness'

test.use({ actionTimeout: 15_000 })

const MARKER = /reported already resolved in round 1/i
const PLAN = `# Plan\n\n${Array.from({ length: 60 }, (_, i) => `plan${i}`).join(' ')}\n`

/** The persisted error is `run_state.snapshot.lastError` (not a top-level field); a missing snapshot fails the check. */
function expectNoMalformedLastError(runState: any) {
  expect(runState?.snapshot, 'run_state.snapshot is present').toBeTruthy()
  expect(JSON.stringify(runState.snapshot.lastError ?? null), 'run_state.snapshot.lastError is not malformed').not.toMatch(/malformed/i)
}

// Negative control (no app): the helper looks at the right field.
test('VAL-R52ui control: the run_state check rejects a malformed snapshot.lastError and a missing snapshot', () => {
  const malformed = { kind: 'malformed', message: 'Invalid structured output: missing response. Diagnostic body omitted.' }
  expect(() => expectNoMalformedLastError({ snapshot: { lastError: malformed } })).toThrow()
  expect(() => expectNoMalformedLastError({ lastError: null })).toThrow()
  expect(() => expectNoMalformedLastError({})).toThrow()
  expect(() => expectNoMalformedLastError({ snapshot: { lastError: null } })).not.toThrow()
  expect(() => expectNoMalformedLastError({ snapshot: {} })).not.toThrow()
})

test('VAL-R52ui: a new issue reported resolved is listed in S7 and in final_review.html with its arrival round', async () => {
  const dataDir = mkdtempSync(join(tmpdir(), 'symvoulio-e2e-'))
  const app = await launchApp({ dataDir, scenario: 'unknown_resolved' })
  try {
    const p = app.page
    await p.getByTestId('new-review').click()
    await p.getByTestId('s6-name').fill('R52 ui')
    await p.getByTestId('s6-objective').fill('Decide whether the plan is sound.')
    await p.getByTestId('add-criterion').click()
    await p.getByTestId('criterion-0').fill('Totals must reconcile.')
    await p.getByTestId('approve-criteria').click()
    await p.getByTestId('primary-input').setInputFiles({ name: 'plan.md', mimeType: 'application/octet-stream', buffer: Buffer.from(PLAN) })
    await p.getByTestId('policy-toggle').click()
    await p.getByTestId('s6pol-max-1').click()
    await p.getByTestId('start').click()
    await expect(p.locator('[data-screen="S7"]')).toBeVisible()

    // The run ends: a gate stops it (one round), or it completes. It must not sit paused.
    const strip = p.getByTestId('status-strip')
    await expect(strip).toHaveAttribute('data-state', /^(stopped|completed|paused)$/, { timeout: 90_000 })
    const state = await strip.getAttribute('data-state')
    expect(scenariosRun(app).filter((s) => s === 'unknown_resolved').length, 'the scenario must have run (L55)').toBeGreaterThan(0)
    expect(state, 'the run must not be paused by the resolved-on-arrival issue').not.toBe('paused')
    await expect(p.getByText(/malformed/i), 'no malformed error is shown').toHaveCount(0)

    // Browser-free: the run folder.
    const root = join(app.workspace, 'runs')
    const runDir = join(root, readdirSync(root).filter((n) => statSync(join(root, n)).isDirectory())[0])
    const runState = JSON.parse(readFileSync(join(runDir, 'run_state.json'), 'utf8'))
    expectNoMalformedLastError(runState)
    const htmlFile = join(runDir, 'output', 'final', 'final_review.html')
    expect(existsSync(htmlFile), 'final_review.html exists').toBe(true)
    expect(readFileSync(htmlFile, 'utf8'), 'final_review.html says «reported already resolved in round 1»').toMatch(MARKER)

    // S7: the issue is in the workspace list with the marker.
    const list = p.getByTestId('issue-list')
    const row = list.locator('li').filter({ hasText: 'R01-001' })
    await expect(row, 'the resolved-on-arrival issue R01-001 is listed').toBeVisible()
    await expect(row, 'the R01-001 row says «reported already resolved in round 1»').toContainText(MARKER)
    await expect(list.locator('li').filter({ hasText: MARKER }), 'exactly one row carries the marker (the five open-on-arrival issues must not)').toHaveCount(1)
  } finally {
    await app.stop()
  }
})
