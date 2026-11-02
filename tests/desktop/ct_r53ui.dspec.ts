// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// CARD R53 (cruel tester): VAL-R53ui. In round 1 the author raises two human questions (R01-001, R01-002) and the
// reviewer, which does not see them, reports a NEW issue R01-001 (the real canary #3 answer, fixture
// r53_real_review.json). The run must not pause or show a malformed error; S7 lists the finding AND shows both questions;
// final/final_review.html carries both texts. Knobs: FAKE_CLI_AUTHOR_QUESTIONS (author), FAKE_CLI_REVIEWER_FILE (reviewer).
import { test, expect } from '@playwright/test'
import { existsSync, mkdtempSync, readdirSync, readFileSync, statSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { launchApp, scenariosRun } from './harness'

test.use({ actionTimeout: 15_000 })

const PLAN = `# Plan\n\n${Array.from({ length: 60 }, (_, i) => `plan${i}`).join(' ')}\n`
const REVIEW = resolve(process.cwd(), 'crates', 'symvoulio-core', 'tests', 'fixtures', 'r53_real_review.json')
const FINDING = 'Both decision-relevant figures lack supporting evidence'
const Q1 = 'Who approves the capital budget?'
const Q2 = 'Which currency do the totals use?'

/** The persisted error is `run_state.snapshot.lastError` (not a top-level field); a missing snapshot fails the check. */
function expectNoMalformedLastError(runState: any) {
  expect(runState?.snapshot, 'run_state.snapshot is present').toBeTruthy()
  expect(JSON.stringify(runState.snapshot.lastError ?? null), 'run_state.snapshot.lastError is not malformed').not.toMatch(/malformed/i)
}

// Negative control (no app): the helper looks at the right field.
test('VAL-R53ui control: the run_state check rejects a malformed snapshot.lastError and a missing snapshot', () => {
  const malformed = { kind: 'malformed', message: 'Invalid structured output: conflicting question id R01-001. Diagnostic body omitted.' }
  expect(() => expectNoMalformedLastError({ snapshot: { lastError: malformed } })).toThrow()
  expect(() => expectNoMalformedLastError({ lastError: null })).toThrow()
  expect(() => expectNoMalformedLastError({})).toThrow()
  expect(() => expectNoMalformedLastError({ snapshot: { lastError: null } })).not.toThrow()
  expect(() => expectNoMalformedLastError({ snapshot: {} })).not.toThrow()
})

test('VAL-R53ui: an author question and a reviewer finding with the same id are both shown, and the run is not paused', async () => {
  const dataDir = mkdtempSync(join(tmpdir(), 'symvoulio-e2e-'))
  const app = await launchApp({
    dataDir,
    // The default scenario ("") runs; the knobs below are the only deviation (asserted after the run).
    env: {
      FAKE_CLI_REVIEWER_FILE: REVIEW,
      FAKE_CLI_AUTHOR_QUESTIONS: JSON.stringify([[{ issue_id: 'R01-001', question: Q1 }, { issue_id: 'R01-002', question: Q2 }]]),
    },
  })
  try {
    const p = app.page
    await p.getByTestId('new-review').click()
    await p.getByTestId('s6-name').fill('R53 ui')
    await p.getByTestId('s6-objective').fill('Decide whether the plan is sound.')
    await p.getByTestId('add-criterion').click()
    await p.getByTestId('criterion-0').fill('Totals must reconcile.')
    await p.getByTestId('approve-criteria').click()
    await p.getByTestId('primary-input').setInputFiles({ name: 'plan.md', mimeType: 'application/octet-stream', buffer: Buffer.from(PLAN) })
    await p.getByTestId('policy-toggle').click()
    await p.getByTestId('s6pol-max-1').click()
    await p.getByTestId('start').click()
    await expect(p.locator('[data-screen="S7"]')).toBeVisible()

    const strip = p.getByTestId('status-strip')
    await expect(strip).toHaveAttribute('data-state', /^(stopped|completed|paused)$/, { timeout: 90_000 })
    const state = await strip.getAttribute('data-state')

    // L55: the knobs ran. The reviewer file and the author questions are visible in the run's own files below; here the
    // provider calls happened with the default scenario (no override leaked in).
    const ran = scenariosRun(app)
    expect(ran.length, 'the fake CLI served provider calls').toBeGreaterThan(0)
    expect(ran.every((s) => s === ''), `the default scenario ran, got ${JSON.stringify(ran)}`).toBe(true)

    expect(state, 'the run must not be paused by the id collision').not.toBe('paused')
    await expect(p.getByText(/malformed/i), 'no malformed error is shown').toHaveCount(0)
    await expect(p.getByText(/invalid structured output/i), 'no rejection text is shown').toHaveCount(0)

    // Browser-free: the run folder.
    const root = join(app.workspace, 'runs')
    const runDir = join(root, readdirSync(root).filter((n) => statSync(join(root, n)).isDirectory())[0])
    const runState = JSON.parse(readFileSync(join(runDir, 'run_state.json'), 'utf8'))
    expectNoMalformedLastError(runState)
    const raw = JSON.stringify(runState.snapshot)
    expect(raw, 'the knobs ran: the author question is in the run').toContain(Q1)
    expect(raw, 'the knobs ran: the reviewer finding is in the run').toContain(FINDING)
    // The human-questions panel (HumanQuestionList.tsx) renders only the question text, never an id, so nothing about ids is
    // asserted on screen. The ids are asserted in the run folder instead: questions are Q<round>-<nnn>, the finding is R01-001.
    const qIds: string[] = runState.snapshot.humanQuestions.map((q: { issue_id: string }) => q.issue_id)
    expect(qIds.length, 'two questions are recorded').toBe(2)
    for (const id of qIds) expect(id, `question id ${id} is in the Q namespace`).toMatch(/^Q[0-9]{2}-[0-9]{3}$/)
    expect(new Set(qIds).size, 'the question ids are distinct').toBe(2)
    expect(
      runState.snapshot.issues.some((i: { issue: { issue_id: string; finding: string } }) => i.issue.issue_id === 'R01-001' && i.issue.finding.includes(FINDING)),
      'the reviewer issue keeps R01-001',
    ).toBe(true)
    const htmlFile = join(runDir, 'output', 'final', 'final_review.html')
    expect(existsSync(htmlFile), 'final_review.html exists').toBe(true)
    const html = readFileSync(htmlFile, 'utf8')
    for (const t of [FINDING, Q1, Q2]) expect(html, `final_review.html contains «${t}»`).toContain(t)

    // S7: the finding is in the issue list; the questions are in the human questions block.
    const list = p.getByTestId('issue-list')
    await expect(list, 'the reviewer finding is listed in S7').toContainText(FINDING)
    await expect(list, 'the question text is not in the issue list').not.toContainText(Q1)
    const questions = p.getByTestId('human-questions').first()
    await expect(questions, 'the first question is shown').toContainText(Q1)
    await expect(questions, 'the second question is shown').toContainText(Q2)
    await expect(questions, 'the finding text is not among the questions').not.toContainText(FINDING)
  } finally {
    await app.stop()
  }
})
