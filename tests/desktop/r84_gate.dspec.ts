// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { test, expect } from '@playwright/test'
import { existsSync, readFileSync } from 'node:fs'
import { execFileSync } from 'node:child_process'
import { join } from 'node:path'
import { launchApp, relaunch, ipc, BASE, type App } from './harness'
import type { RunSnapshot } from '../../src/services/types'

test.use({ actionTimeout: 15_000 })
const snapshot = (a: App, id: string) => ipc<RunSnapshot>(a.page, 'run_get', { runId: id })
async function start(a: App) {
  const p = a.page
  await p.getByTestId('nav-new').click()
  await p.getByTestId('s6-name').fill('Final gate review')
  await p.getByTestId('s6-objective').fill('Check facts')
  await p.getByTestId('add-criterion').click()
  await p.getByTestId('criterion-0').fill('Facts supported')
  await p.getByTestId('approve-criteria').click()
  await p.getByTestId('policy-toggle').click()
  await p.getByTestId('s6pol-max-3').click()
  await p.getByTestId('primary-input').setInputFiles({ name: 'plan.md', mimeType: 'text/markdown', buffer: Buffer.from('# Plan\n\n' + Array.from({ length: 100 }, (_, i) => `word${i}`).join(' ')) })
  await p.getByTestId('start').click()
  await expect(p.locator('[data-screen="S7"]')).toBeVisible()
  await expect.poll(async () => (await ipc<{ id: string }[]>(p, 'runs_list')).length).toBe(1)
  return (await ipc<{ id: string }[]>(p, 'runs_list'))[0].id
}

async function restartPaused(a: App, id: string) {
  expect(execFileSync('powershell.exe', ['-NoProfile', '-Command', `(Get-Process -Id ${a.proc.pid}).CloseMainWindow()`], { encoding: 'utf8' }).trim()).toBe('True')
  await expect.poll(() => a.proc.exitCode, { timeout: 15_000 }).not.toBeNull()
  const next = await relaunch(a)
  await next.page.goto(`${BASE}/review/${id}`)
  await expect(next.page.getByTestId('resume')).toBeEnabled()
  return next
}

const escapeHtml = (s: string) => s.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;').replaceAll("'", '&#39;')
async function assertFullPackage(a: App, s: RunSnapshot) {
  await a.page.getByTestId('export').click()
  await a.page.getByTestId('do-export').click()
  await expect(a.page.getByTestId('export-result')).toContainText('Exported')
  const path = (await a.page.getByTestId('run-path').innerText()).trim()
  for (const file of ['final/final_document.md', 'final/final_document.docx', 'final/final_review.html', 'audit/run_manifest.json', ...[1, 2, 3].flatMap(n => [`reviews/author_response_0${n}.json`, `reviews/review_0${n}.json`, `revisions/revision_0${n}.md`])]) {
    expect(existsSync(join(path, file)), file).toBe(true)
  }
  const html = readFileSync(join(path, 'final/final_review.html'), 'utf8')
  expect(html).toContain('<h2>Author responses</h2>')
  const responses = s.issues.flatMap(i => i.authorResponses.map(r => ({ id: i.issue.issue_id, ...r })))
  expect(responses.length).toBeGreaterThan(0)
  expect(new Set(responses.map(r => r.response))).toEqual(new Set(['accepted', 'rejected_with_evidence', 'partially_accepted', 'human_decision_required']))
  for (const r of responses) {
    expect(html).toContain(`<td>${r.id}</td><td>${r.round}</td><td>${r.response}</td><td>${escapeHtml(r.explanation)}</td><td>${escapeHtml(r.evidence)}</td>`)
  }
}

for (const round of [1, 2, 3]) for (const restart of [false, true]) {
  test(`pause in round ${round}, ${restart ? 'restart then ' : ''}resume preserves final human gate and full export`, async () => {
    let a = await launchApp({ env: { FAKE_CLI_DELAY_MS: '1500', FAKE_CLI_AUTHOR_QUESTIONS: JSON.stringify([Array.from({ length: 7 }, (_, i) => ({ issue_id: `R01-${i + 1}`, question: `Who verifies fact ${i + 1}?` }))]) } })
    try {
      const id = await start(a)
      await expect.poll(async () => { const s = await snapshot(a, id); return [s.roundsDone + 1, s.phase] }, { timeout: 60_000 }).toEqual([round, 'reviewer'])
      await a.page.getByTestId('pause').click()
      await expect.poll(async () => (await snapshot(a, id)).state).toBe('paused')
      const paused = await snapshot(a, id)
      expect(paused.roundsDone).toBe(round)
      if (restart) a = await restartPaused(a, id)
      await expect(a.page.getByTestId('status-state')).toHaveText('Paused')
      await a.page.getByTestId('resume').click()
      await expect(a.page.getByTestId('status-state')).toHaveText('Completed', { timeout: 60_000 })
      const s = await snapshot(a, id)
      expect(s.roundsDone).toBe(3)
      expect(s.stopReason).toBe('gate_human_review_required')
      expect(s.rounds.at(-1)?.gate.reason).toBe('stopped_human')
      expect(s.humanQuestions.filter(q => !q.resolved)).toHaveLength(7)
      expect(s.rounds.at(-1)?.contributionExceeded).toBe(true)
      // Each author/reviewer call occurs once; resuming a saved final gate must not call either provider again.
      // Concurrent CLI detection can interleave log writes; count the unique call-option tokens.
      const calls = readFileSync(a.fakeLog, 'utf8').match(/"--(?:json|output)-schema"/g) ?? []
      expect(calls).toHaveLength(6)
      console.log(JSON.stringify({ round, restart, state: s.state, pausedRounds: paused.roundsDone, finalGate: s.rounds.at(-1)?.gate, questions: s.humanQuestions.length }))
      for (const n of [1, 2, 3]) {
        await a.page.getByTestId(`round-${n}-toggle`).click()
        const replies = s.issues.flatMap(i => i.authorResponses.filter(r => r.round === n))
        const count = (kind: string) => replies.filter(r => r.response === kind).length
        await expect(a.page.getByTestId(`round-${n}-responses`)).toHaveText(`Author responses: ${count('accepted')} accepted, ${count('rejected_with_evidence')} rejected with evidence, ${count('partially_accepted')} partially accepted, ${count('human_decision_required')} for human decision`)
      }
      await a.page.getByTestId('human-decision').click()
      await expect(a.page.getByTestId('main')).not.toContainText('Review stopped')
      await expect(a.page.getByTestId('reviewer-status')).toHaveText('Human review required')
      await expect(a.page.getByTestId('final-gate-reason')).toContainText('round limit')
      if (round === 3 && !restart) for (const [width, height] of [[1366, 768], [1920, 1080], [390, 844]]) {
        await a.page.setViewportSize({ width, height })
        const card = a.page.getByTestId('not-approval')
        await expect(card).toBeInViewport({ ratio: 1 })
        const box = await card.boundingBox()
        console.log(`warning ${width}x${height}: ${JSON.stringify(box)}`)
        expect(box!.y).toBeGreaterThanOrEqual(0)
        expect(box!.y + box!.height).toBeLessThanOrEqual(height)
        await a.page.screenshot({ path: test.info().outputPath(`decision-${width}.png`) })
      }
      await a.page.getByTestId('back-to-workspace').click()
      await assertFullPackage(a, s)
    } finally { await a.stop() }
  })
}

test('stopped then rejected review exports an existing partial package', async () => {
  const a = await launchApp({ env: { FAKE_CLI_DELAY_MS: '2000' } })
  try {
    const id = await start(a)
    await expect.poll(async () => (await snapshot(a, id)).phase).toBe('reviewer')
    await a.page.getByTestId('stop').click()
    await expect.poll(async () => (await snapshot(a, id)).state).toBe('stopped')
    expect((await snapshot(a, id)).roundsDone).toBe(0)
    await a.page.getByTestId('human-decision').click()
    await a.page.getByTestId('reject').click()
    await a.page.getByTestId('decider-name').fill('QA')
    await a.page.getByTestId('confirm').click()
    await a.page.getByTestId('decided-export').click()
    await a.page.getByTestId('export-partial').click()
    await expect(a.page.getByTestId('export-result')).toContainText('Partial')
    const path = (await a.page.getByTestId('run-path').innerText()).trim()
    expect(readFileSync(join(path, 'final/human_decision.json'), 'utf8')).toContain('rejected')
    expect(existsSync(join(path, 'revisions/revision_01.md'))).toBe(true)
    expect(existsSync(join(path, 'final/final_review.html'))).toBe(true)
  } finally { await a.stop() }
})
