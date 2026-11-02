// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// CARD AST-A: the external review's pass-2 desktop-workflow failures, reproduced through the real debug app (real IPC, real engine,
// the fake provider CLI) over WebView2 CDP. One test per VAL; each test name starts with its VAL id.
// Opt-in fake-CLI knobs used here (crates/symvoulio-core/src/bin/fake-cli.rs): FAKE_CLI_STDIN_LOG, FAKE_CLI_SCENARIO_AUTHOR,
// FAKE_CLI_SCENARIO_REVIEWER, FAKE_CLI_DELAY_MS, FAKE_CLI_SLOW_ROUND, scenarios `rewrite` and `high_human`.
import { test, expect, type Page } from '@playwright/test'
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { launchApp, relaunch, ipc, scenariosRun, type App } from './harness'

test.use({ actionTimeout: 15_000 })

// ---------------------------------------------------------------- helpers
async function launch(env: Record<string, string> = {}, scenario?: string): Promise<App> {
  const dataDir = mkdtempSync(join(tmpdir(), 'symvoulio-e2e-'))
  writeFileSync(join(dataDir, 'stdin.log'), '')
  return launchApp({ dataDir, scenario, env: { FAKE_CLI_STDIN_LOG: join(dataDir, 'stdin.log'), ...env } })
}
const prompts = (app: App): string[] =>
  readFileSync(join(app.dataDir, 'stdin.log'), 'utf8').split('\n').filter(Boolean).map((l) => JSON.parse(l) as string)
const isAuthorPrompt = (p: string) => p.includes('<<<OPEN_FINDINGS')
const runIds = (app: App): string[] => {
  const dir = join(app.workspace, 'runs')
  return existsSync(dir) ? readdirSync(dir).filter((n) => statSync(join(dir, n)).isDirectory()) : []
}
const runDir = (app: App) => join(app.workspace, 'runs', runIds(app)[0])
const filesUnder = (dir: string): string[] =>
  !existsSync(dir) ? [] : readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? filesUnder(join(dir, e.name)) : [join(dir, e.name)]))
const strip = (page: Page) => page.getByTestId('status-strip')

const words = (n: number, prefix = 'alpha') => Array.from({ length: n }, (_, i) => `${prefix}${i}`).join(' ')
const PLAN = `# Plan\n\n${words(60, 'plan')}\n`

interface StartOpts {
  name?: string
  primary?: { name: string; text: string | Buffer }
  supporting?: { name: string; text: string | Buffer }[]
  maxRounds?: number
  newTextZero?: boolean
}
/** Drive S11 → S6: fill the form, approve the criteria, press «Start deliberation». */
async function startReview(app: App, o: StartOpts = {}) {
  const p = app.page
  await p.getByTestId('new-review').click()
  await p.getByTestId('s6-name').fill(o.name ?? 'CT review')
  await p.getByTestId('s6-objective').fill('Decide whether the plan is sound.')
  await p.getByTestId('add-criterion').click()
  await p.getByTestId('criterion-0').fill('Totals must reconcile.')
  await p.getByTestId('approve-criteria').click()
  const primary = o.primary ?? { name: 'plan.md', text: PLAN }
  await p.getByTestId('primary-input').setInputFiles({ name: primary.name, mimeType: 'application/octet-stream', buffer: Buffer.from(primary.text) })
  for (const s of o.supporting ?? []) {
    await p.getByTestId('supporting-input').setInputFiles({ name: s.name, mimeType: 'application/octet-stream', buffer: Buffer.from(s.text) })
  }
  if (o.maxRounds !== undefined || o.newTextZero) {
    await p.getByTestId('policy-toggle').click()
    if (o.maxRounds !== undefined) await p.getByTestId(`s6pol-max-${o.maxRounds}`).click()
    if (o.newTextZero) await p.getByTestId('s6pol-new').getByText('0 %', { exact: true }).click()
  }
  await p.getByTestId('start').click()
  await expect(p.locator('[data-screen="S7"]')).toBeVisible()
}
const waitState = (app: App, state: string, timeout = 90_000) => expect(strip(app.page)).toHaveAttribute('data-state', state, { timeout })
const waitPrompts = (app: App, n: number, timeout = 30_000) => expect.poll(() => prompts(app).length, { timeout }).toBeGreaterThanOrEqual(n)
/** L55: the named fake scenario (for a role override: that role's scenario name) really ran for at least one provider call. */
const expectScenario = (app: App, name: string) => expect.poll(() => scenariosRun(app), { message: `L55: the scenario «${name}» ran at least once`, timeout: 60_000 }).toContain(name)

// ---------------------------------------------------------------- VAL-R07a
test('VAL-R07a: Pause (worker exits) then Stop in the UI shows Stopped, Human decision and Export enabled, no reload', async () => {
  const app = await launch({ FAKE_CLI_DELAY_MS: '1500' })
  try {
    await startReview(app, { name: 'R07a' })
    await waitPrompts(app, 1)
    await app.page.getByTestId('pause').click()
    await waitState(app, 'paused', 30_000)
    await app.page.getByTestId('stop').click()
    await expect(strip(app.page)).toHaveAttribute('data-state', 'stopped', { timeout: 10_000 })
    await expect(app.page.getByTestId('status-state')).toHaveText('Stopped')
    await expect(app.page.getByTestId('human-decision')).toBeEnabled()
    await expect(app.page.getByTestId('export')).toBeEnabled()
  } finally { await app.stop() }
})

// ---------------------------------------------------------------- VAL-R07b
test('VAL-R07b: Stop right after the worker emitted Paused ends in Stopped, never stays Paused (24 runs)', async () => {
  const app = await launch({ FAKE_CLI_DELAY_MS: '200' })
  try {
    const N = 60
    const out = await app.page.evaluate(async (n) => {
      const inv = (c: string, a: Record<string, unknown>) => (window as any).__TAURI_INTERNALS__.invoke(c, a)
      const ti = (window as any).__TAURI_INTERNALS__
      const policy = { max_rounds: 3, min_rounds: 1, contribution: 'correct', new_content_limit_pct: 100, stop: { no_critical: true, no_high: true, no_new: true, max_rounds: true } }
      const bytes = Array.from(new TextEncoder().encode('# Doc\n\nsome words in a short document for the race test\n'))
      // All runs are started and paused first, then watched concurrently: the many workers and pollers compete for the CPU,
      // which widens the window between a worker saving Paused and its thread exiting; each Stop lands the moment Paused is seen.
      const ids: string[] = []
      for (let i = 0; i < n; i++) {
        const f = await inv('stage_file', { name: `d${i}.md`, bytes })
        const snap = await inv('run_start', { input: { name: `race ${i}`, objective: 'o', criteria: ['c'], policy, author: 'claude', primaryToken: f.token, supportingTokens: [] }, onEvent: `__CHANNEL__:${ti.transformCallback(() => {})}` })
        await inv('run_pause', { runId: snap.id })
        ids.push(snap.id)
      }
      return Promise.all(ids.map(async (id) => {
        let sawPaused = false, stopError: string | null = null
        const t0 = Date.now()
        while (Date.now() - t0 < 30_000) {
          const s = await inv('run_get', { runId: id })
          if (s.state === 'paused') {
            sawPaused = true
            try { await inv('run_stop', { runId: id }) } catch (e) { stopError = String(e) }
            break
          }
          if (s.state !== 'running') break
        }
        return { id, sawPaused, stopError }
      }))
    }, N)
    expect(out.filter((r) => !r.sawPaused), 'every run reached Paused after the pause request').toEqual([])
    expect(out.filter((r) => r.stopError)).toEqual([])
    const finals: Record<string, string> = {}
    for (const r of out) {
      const end = Date.now() + 5_000
      let state = ''
      while (Date.now() < end) {
        state = (await ipc<{ state: string }>(app.page, 'run_get', { runId: r.id })).state
        if (state !== 'paused') break
      }
      finals[r.id] = state
    }
    expect(Object.values(finals).filter((s) => s !== 'stopped'), `final states: ${JSON.stringify(finals)}`).toEqual([])
  } finally { await app.stop() }
})

// ---------------------------------------------------------------- VAL-R09
test('VAL-R09: «Accept the finding» on an unresolved High keeps it open (S7, S9, ledger); only «Keep the text» closes it', async () => {
  const app = await launch({ FAKE_CLI_SCENARIO_REVIEWER: 'high_human' })
  try {
    const p = app.page
    await startReview(app, { name: 'R09', maxRounds: 1 })
    await expectScenario(app, 'high_human')
    await waitState(app, 'completed')
    const high = () => p.getByTestId('count-high-n')
    await expect(high()).toHaveText('2')

    // Accept R01-002 in the S8 drawer.
    await p.getByTestId('issue-R01-002').click()
    await expect(p.getByTestId('issue-drawer')).toBeVisible()
    await p.getByTestId('decide-accept').click()
    await p.getByTestId('decide-note').fill('implement it')
    await p.getByTestId('decide').click()
    await expect(p.getByTestId('decided')).toBeVisible()
    await p.keyboard.press('Escape')
    await expect(p.getByTestId('issue-drawer')).toHaveCount(0)
    await expect(high(), 'Accept is not a waiver: both High issues stay open').toHaveText('2')
    await expect(p.getByTestId('issue-R01-002').getByTestId('issue-status')).not.toHaveText(/resolved/i)

    // Keep R01-003 (the explicit waiver): only now a High closes.
    await p.getByTestId('issue-R01-003').click()
    await expect(p.getByTestId('issue-drawer')).toBeVisible()
    await p.getByTestId('decide-keep').click()
    await p.getByTestId('decide').click()
    await expect(p.getByTestId('decided')).toBeVisible()
    await p.keyboard.press('Escape')
    await expect(high()).toHaveText('1')

    // S9: High >= 1 and the threshold Failed, equal to the backend ledger.
    await p.getByTestId('human-decision').click()
    await expect(p.locator('[data-screen="S9"]')).toBeVisible()
    await expect(p.getByTestId('threshold')).toHaveText('Failed')
    const shown = Number(/\d+/.exec(await p.getByTestId('k-high').innerText())![0])
    expect(shown).toBeGreaterThanOrEqual(1)
    const ledger = await ipc<{ counts: { high: number } }>(p, 'run_get', { runId: runIds(app)[0] })
    expect(shown, 'S9 High equals the backend ledger count').toBe(ledger.counts.high)
    expect(ledger.counts.high).toBe(1)
  } finally { await app.stop() }
})

// ---------------------------------------------------------------- VAL-R10
test('VAL-R10: an author question is answered in the UI; the answer persists, survives a restart and reaches the next author prompt', async () => {
  const app = await launch({ FAKE_CLI_SCENARIO_AUTHOR: 'human_question', FAKE_CLI_DELAY_MS: '1500' })
  let current = app
  try {
    await startReview(app, { name: 'R10' })
    await expectScenario(app, 'human_question')
    await waitPrompts(app, 1)
    await app.page.getByTestId('pause').click()
    await waitState(app, 'paused', 30_000)
    const questions = () => current.page.getByTestId('human-questions').first()
    await expect(questions()).toContainText('Who owns the capital budget?')

    // Deciding without an answer must not mark the question resolved (a short, bounded wait lets a wrong "resolved" appear).
    // Compliant either way: Decide is disabled while the answer is empty, or clicking it leaves the question unresolved.
    const decide = questions().getByRole('button').first()
    if (await decide.isEnabled()) {
      await decide.click()
      await current.page.waitForTimeout(1500)
    }
    expect.soft(await questions().innerText(), 'no answer given: the question stays unresolved').not.toMatch(/resolved/i)

    // Answer it.
    await questions().getByRole('textbox').fill('Finance')
    await questions().getByRole('button').first().click()
    await expect(questions()).toContainText('Finance')
    // Persisted on the question record.
    expect(readFileSync(join(runDir(app), 'run_state.json'), 'utf8'), 'the answer is stored with the question').toContain('Finance')

    // Restart the app: the answer is still shown.
    const id = runIds(app)[0]
    current = await relaunch(app)
    await current.page.getByTestId(`run-${id}`).getByTestId('open').click()
    await expect(questions()).toContainText('Who owns the capital budget?')
    await expect(questions()).toContainText('Finance')

    // The next author prompt carries the answer.
    const before = prompts(app).length
    await current.page.getByTestId('resume').click()
    await expect.poll(() => prompts(app).slice(before).filter(isAuthorPrompt).some((q) => q.includes('Finance')), { timeout: 60_000 }).toBe(true)
  } finally { await current.stop() }
})

// ---------------------------------------------------------------- VAL-R11
test('VAL-R11: with a 0 % new-text limit the contribution warning in final_review.html survives Export in S10', async () => {
  const app = await launch()
  try {
    const p = app.page
    await startReview(app, { name: 'R11', maxRounds: 1, newTextZero: true })
    await waitState(app, 'completed')
    const file = join(runDir(app), 'output', 'final', 'final_review.html')
    expect(readFileSync(file, 'utf8'), 'precondition: the warning is in the report before export').toContain('Contribution limit exceeded')
    await p.getByTestId('export').click()
    await expect(p.locator('[data-screen="S10"]')).toBeVisible()
    await p.getByTestId('do-export').click()
    await expect(p.getByTestId('export-result')).toBeVisible()
    expect(readFileSync(file, 'utf8'), 'the warning is still in the report after Export').toContain('Contribution limit exceeded')
  } finally { await app.stop() }
})

// ---------------------------------------------------------------- VAL-R14
test('VAL-R14: Restart from revision keeps the supporting file (S6 chip, run inputs, provider prompt)', async () => {
  const MARK = 'ZEBRA-SUPPORT-7731'
  const app = await launch()
  try {
    const p = app.page
    await startReview(app, { name: 'R14', maxRounds: 1, supporting: [{ name: 'notes.md', text: `# Notes\n\nThe budget ceiling is 12 units. ${MARK}\n` }] })
    await waitState(app, 'completed')
    expect(prompts(app).some((q) => q.includes(MARK)), 'precondition: the first run sent the supporting text').toBe(true)
    const first = runIds(app)[0]

    await p.getByTestId('nav-reviews').click()
    // A run started in this UI session is listed under the store's local id: find its row by the run name.
    await p.locator('tr', { hasText: 'R14' }).getByTestId('restart').click()
    await p.getByTestId('rev-1').click()
    await p.getByTestId('restart-go').click()
    await expect(p.getByTestId('restart-badge')).toBeVisible()
    await expect.soft(p.getByTestId('sup-chip'), 'the restart form shows the supporting file').toHaveCount(1)
    await expect.soft(p.getByTestId('sup-chip')).toContainText('notes.md')
    await p.getByTestId('approve-criteria').click()
    const sent = prompts(app).length
    await p.getByTestId('start').click()
    await expect.poll(() => runIds(app).length, { timeout: 30_000 }).toBe(2)
    await waitState(app, 'completed')

    const second = runIds(app).find((r) => r !== first)!
    const snap = await ipc<{ supportingNames: string[] }>(p, 'run_get', { runId: second })
    expect(snap.supportingNames, 'the restarted run keeps the supporting input').toContain('notes.md')
    expect(prompts(app).slice(sent).some((q) => q.includes(MARK)), 'the restarted run sends the supporting text to the provider').toBe(true)
  } finally { await app.stop() }
})

// ---------------------------------------------------------------- VAL-R18
test('VAL-R18: the Diff tab on ~10,000 rewritten words keeps the UI responsive (round trip < 1 s) and shows within 15 s', async () => {
  const app = await launch({ FAKE_CLI_SCENARIO_AUTHOR: 'rewrite' })
  try {
    const p = app.page
    await startReview(app, { name: 'R18', maxRounds: 1, primary: { name: 'big.md', text: `# Big document\n\n${words(10_000)}\n` } })
    await expectScenario(app, 'rewrite')
    await waitState(app, 'completed', 180_000)
    await p.evaluate(() => {
      const w = window as any
      w.__gap = 0
      let last = performance.now()
      setInterval(() => { const n = performance.now(); w.__gap = Math.max(w.__gap, n - last); last = n }, 50)
    })
    const box = (await p.getByTestId('tab-diff').boundingBox())!
    await p.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
    const t0 = Date.now()
    const down = p.mouse.down() // the Diff tab activates on mouse down
    const roundTrip = p.evaluate(() => 1).then(() => Date.now() - t0)
    await down
    await p.mouse.up()
    await expect(p.getByTestId('diff-side')).toBeVisible({ timeout: 15_000 })
    const rt = await roundTrip
    const gap = await p.evaluate(() => (window as any).__gap as number)
    expect(rt, `page.evaluate round trip while the diff is computed: ${rt} ms`).toBeLessThan(1000)
    expect(gap, `longest main-thread stall: ${Math.round(gap)} ms`).toBeLessThan(1000)
  } finally { await app.stop() }
})

// ---------------------------------------------------------------- VAL-R29
test('VAL-R29: storage failing after start does not leave the UI at «Running»', async () => {
  const app = await launch({ FAKE_CLI_DELAY_MS: '5000' })
  try {
    const p = app.page
    await startReview(app, { name: 'R29' })
    await waitPrompts(app, 1)
    // While the (slow) provider call runs, the run's state file becomes unreadable and unwritable: a directory takes its place.
    const state = join(runDir(app), 'run_state.json')
    rmSync(state)
    mkdirSync(state)
    // The call ends ~5 s after it began; then the UI has 30 s to show an error, or a paused or failed state.
    await expect.poll(async () => {
      const s = await strip(p).getAttribute('data-state')
      const alerts = await p.locator('[role="alert"]:visible').count()
      const badge = (await p.getByTestId('run-state').first().innerText().catch(() => '')).trim()
      return alerts > 0 || s === 'paused' || s === 'stopped' || badge === 'Failed' || badge === 'Paused'
    }, { timeout: 40_000, intervals: [500] }).toBe(true)
  } finally { await app.stop() }
})

// ---------------------------------------------------------------- VAL-R15
test('VAL-R15: a failed approval shows an error on S9 and keeps the decider name and note', async () => {
  const app = await launch()
  try {
    const p = app.page
    await startReview(app, { name: 'R15', maxRounds: 1 })
    await waitState(app, 'completed')
    await p.getByTestId('human-decision').click()
    await expect(p.locator('[data-screen="S9"]')).toBeVisible()
    // The run folder cannot be written: its state file's temporary name is taken by a directory.
    mkdirSync(join(runDir(app), 'run_state.json.tmp'))
    await p.getByTestId('approve').click()
    await p.getByTestId('decider-name').fill('Dana Reviewer')
    await p.getByTestId('decider-note').fill('Checked against the ledger')
    await p.getByTestId('confirm').click()
    await expect(p.locator('[data-screen="S9"] [role="alert"], [data-testid="confirm-dialog"] [role="alert"]').first(), 'the failure is shown on S9').toBeVisible({ timeout: 15_000 })
    await expect(p.getByTestId('decided')).toHaveCount(0)
    // The entered values are kept (the dialog stays open, or reopens with them).
    if (!(await p.getByTestId('decider-name').isVisible())) await p.getByTestId('approve').click()
    await expect(p.getByTestId('decider-name')).toHaveValue('Dana Reviewer')
    await expect(p.getByTestId('decider-note')).toHaveValue('Checked against the ledger')
  } finally { await app.stop() }
})

test('VAL-R15: a failed settings write shows an error on S12 and the displayed value rolls back', async () => {
  const app = await launch()
  try {
    const p = app.page
    await p.getByTestId('nav-settings').click()
    await expect(p.locator('[data-screen="S12"]')).toBeVisible()
    const select = p.getByTestId('shell-claude')
    const before = await select.inputValue()
    const target = before === 'pwsh' ? 'powershell' : 'pwsh'
    // settings.json cannot be written: its temporary name is taken by a directory.
    mkdirSync(join(app.dataDir, 'settings.tmp'))
    await select.selectOption(target)
    await expect(p.locator('[data-screen="S12"] [role="alert"]').first(), 'the failure is shown on S12').toBeVisible({ timeout: 15_000 })
    await expect(select, 'the displayed value rolls back').toHaveValue(before)
  } finally { await app.stop() }
})

// ---------------------------------------------------------------- VAL-R30
test('VAL-R30: while the reviewer reviews revision N, Current shows revision N; Final shows the last committed revision', async () => {
  const app = await launch({ FAKE_CLI_SCENARIO_REVIEWER: 'slow', FAKE_CLI_SLOW_ROUND: '2' })
  try {
    const p = app.page
    await startReview(app, { name: 'R30' })
    await expectScenario(app, 'slow')
    // Round 2's reviewer call has started (the second reviewer prompt: ROUND 2, no OPEN_FINDINGS block).
    await expect.poll(() => prompts(app).some((q) => q.startsWith('ROUND: 2') && !isAuthorPrompt(q)), { timeout: 60_000 }).toBe(true)
    await p.getByTestId('tab-current').click()
    await expect(p.getByTestId('doc-current-rev'), 'Current shows the author\'s candidate N = 2').toHaveText('Revision 2', { timeout: 15_000 })
    await expect(p.getByTestId('doc-current')).toContainText('revision 2')
    // The fake author appends its marker to the previous text, so revision 2 carries the round-1 marker too: revision 2
    // is the one ending with its own marker (revision 1 never contains it).
    await expect(p.getByTestId('doc-current')).toContainText('<!-- revision 2 -->')

    await p.getByTestId('stop').click()
    await waitState(app, 'stopped', 30_000)
    await p.getByTestId('tab-final').click()
    await expect(p.getByTestId('doc-final'), 'Final is the last committed revision (1)').toContainText('revision 1')
    await expect(p.getByTestId('doc-final')).not.toContainText('revision 2')
  } finally { await app.stop() }
})

// ---------------------------------------------------------------- VAL-R05
test('VAL-R05: a damaged supporting .docx fails the start visibly and leaves no staging copy, also after deleting the run', async () => {
  const app = await launch()
  try {
    const p = app.page
    await startReview(app, { name: 'R05', supporting: [{ name: 'broken.docx', text: 'this is not a zip archive' }] })
    await expect(p.locator('[role="alert"]:visible').first(), 'the start failure is visible').toBeVisible({ timeout: 30_000 })
    const staging = join(app.workspace, 'staging')
    await expect.poll(() => filesUnder(staging), { message: 'no staging copy of either file after the failed start', timeout: 5_000 }).toEqual([])
    await p.getByRole('button', { name: 'Delete', exact: true }).click()
    await expect(p.locator('[data-screen="S11"]')).toBeVisible()
    await expect.poll(() => filesUnder(staging), { message: 'no staging copy after deleting the failed draft', timeout: 5_000 }).toEqual([])
  } finally { await app.stop() }
})

// ---------------------------------------------------------------- VAL-R29b
test('VAL-R29b: an unreadable run state leaves Running within 30 s with a storage error of its own, not an authentication failure', async () => {
  const app = await launch({ FAKE_CLI_DELAY_MS: '5000' })
  try {
    const p = app.page
    await startReview(app, { name: 'R29b' })
    await waitPrompts(app, 1)
    // While the provider call runs the state file becomes unreadable (a directory takes its place): run_get fails afterwards.
    const state = join(runDir(app), 'run_state.json')
    rmSync(state)
    mkdirSync(state)
    // The call ends ~5 s after it began; then the UI has 30 s to leave Running.
    await expect.poll(async () => {
      const s = await strip(p).getAttribute('data-state')
      const badge = (await p.getByTestId('run-state').first().innerText().catch(() => '')).trim()
      return s === 'paused' || badge === 'Failed' || badge === 'Paused'
    }, { message: 'the UI shows paused or failed', timeout: 40_000, intervals: [500] }).toBe(true)
    // A message of its own about storage...
    const messages = (await p.locator('[role="alert"]:visible, [role="status"]:visible').allInnerTexts()).join('\n')
    expect(messages, 'a storage error message is shown').toMatch(/storage|state file|run folder|disk|could not (read|save|write|load)|unreadable/i)
    // ...and never an authentication failure: no wording, no Reconnect (it would reset a live provider connection).
    expect(await p.locator('body').innerText()).not.toMatch(/authentication failed|not signed in/i)
    await expect(p.getByTestId('reconnect')).toHaveCount(0)
    await expect(p.getByRole('button', { name: /reconnect/i })).toHaveCount(0)
    await expect(p.getByTestId('provider-error')).toHaveCount(0)
  } finally { await app.stop() }
})

// ---------------------------------------------------------------- VAL-R02live / VAL-R02stream
// Fake-CLI knobs: FAKE_CLI_LIVE_LINES (JSON array printed as live output before the answer) and scenario `live_error`
// (the lines also go to stderr and the call fails carrying them).
const PROBE = 'LIVEPROBE-77 reviewing section 3 of 7: capital budget'
const SECRET_LINES = [
  PROBE,
  'password=hunter2',
  'Cookie: session=abc123',
  '{"apiKey":"shortSecret"}',
  'Authorization: Basic dXNlcjpwYXNz',
  'token=ghp_short1',
]
const SECRET_VALUES = ['hunter2', 'abc123', 'shortSecret', 'dXNlcjpwYXNz', 'ghp_short1']
/** Every file under the data dir except the browser profile and the test's own logs (stdin.log, fake-cli.log). */
const appFiles = (app: App): string[] => filesUnder(app.dataDir).filter((f) => {
  const rel = f.slice(app.dataDir.length + 1).replace(/\\/g, '/')
  return !rel.startsWith('webview2/') && rel !== 'stdin.log' && rel !== 'fake-cli.log' && statSync(f).size < 50_000_000
})
function leaks(text: string): string[] {
  return SECRET_VALUES.filter((v) => text.includes(v) || text.includes(JSON.stringify(v).slice(1, -1)))
}

// Re-scoped by product decision A (2026-10-31 05:52Z): the default panel shows a controlled summary, raw text only behind the toggle.
const SUMMARY = /assistant text, \d+ words?/i
for (const path of ['success', 'error'] as const) for (const mode of ['default', 'raw'] as const) {
  test(`VAL-R02live: secret shapes in live provider output (${path} path, ${mode} panel) reach neither the UI, run_state.json, output/ nor the app files`, async () => {
    // The scenario goes through launch()'s scenario parameter: the harness rejects FAKE_CLI_SCENARIO passed through env.
    const app = await launch({ FAKE_CLI_LIVE_LINES: JSON.stringify(SECRET_LINES), FAKE_CLI_DELAY_MS: '3000' }, path === 'error' ? 'live_error' : undefined)
    try {
      const p = app.page
      await startReview(app, { name: `R02live ${path} ${mode}`, maxRounds: 1 })
      if (path === 'error') await expectScenario(app, 'live_error')
      if (mode === 'raw') {
        const toggle = p.getByLabel(/Show raw provider output/i)
        if (!(await toggle.isVisible())) await p.locator('summary, button, [role="button"]').filter({ hasText: /raw provider output/i }).first().click()
        await toggle.check()
      }
      let seen = ''
      const sample = async () => { seen += '\n' + (await p.locator('body').innerText().catch(() => '')) }
      if (mode === 'raw' && path === 'error') {
        // The failing call of the error path ends at once, before the toggle can be on: resume the paused run, so that a
        // call is made with raw ON, and watch that call.
        await waitState(app, 'paused', 30_000)
        const n = prompts(app).length
        await p.getByTestId('resume').click()
        await expect.poll(() => prompts(app).length, { timeout: 30_000 }).toBeGreaterThan(n)
        const t0 = Date.now()
        while (Date.now() - t0 < 4_000) { await sample(); await p.waitForTimeout(200) }
      } else {
        const end = Date.now() + 90_000
        while (Date.now() < end) {
          await sample()
          const s = await strip(p).getAttribute('data-state')
          if (path === 'error' ? s === 'paused' : s === 'stopped' || s === 'completed') break
          await p.waitForTimeout(200)
        }
      }
      await sample()
      if (mode === 'raw') expect(seen, 'raw ON: the probe line appears (the test is not vacuous)').toContain('LIVEPROBE-77')
      else {
        expect(seen, 'default panel: a controlled summary event appears during the call (the panel moves)').toMatch(SUMMARY)
        expect.soft(seen, 'default panel: no provider text').not.toContain('LIVEPROBE-77')
      }
      expect.soft(leaks(seen), 'secret values in the UI text').toEqual([])
      // A short settle so the final state is on disk, then the files.
      await expect.poll(() => readFileSync(join(runDir(app), 'run_state.json'), 'utf8').length, { timeout: 5_000 }).toBeGreaterThan(0)
      const hits: string[] = []
      for (const f of appFiles(app)) {
        const found = leaks(readFileSync(f, 'utf8'))
        if (found.length) hits.push(`${f.slice(app.dataDir.length + 1)}: ${found.join(',')}`)
      }
      expect.soft(hits, 'secret values in files (run_state.json, output/, logs)').toEqual([])
    } finally { await app.stop() }
  })
}

// Re-scoped by product decision A (2026-10-31 05:52Z): raw provider text shows only with the raw toggle ON, so it is switched on first.
test('VAL-R02stream: a live provider line reaches the S7 live view as its own text while the call runs', async () => {
  const LINE = 'Reviewing section 3 of 7: capital budget'
  const app = await launch({ FAKE_CLI_LIVE_LINES: JSON.stringify([LINE]), FAKE_CLI_DELAY_MS: '8000' })
  try {
    const p = app.page
    await startReview(app, { name: 'R02stream' })
    const fold = p.locator('summary, button, [role="button"]').filter({ hasText: /raw provider output/i }).first()
    const toggle = p.getByLabel(/Show raw provider output/i)
    if (!(await toggle.isVisible())) await fold.click()
    await toggle.check()
    await waitPrompts(app, 1)
    await expect.poll(async () => (await p.locator('body').innerText()).includes(LINE), { message: 'the line is in the live view', timeout: 30_000, intervals: [250] }).toBe(true)
    // It arrived while the call was still running, not after the run.
    await expect(strip(p)).toHaveAttribute('data-state', 'running')
  } finally { await app.stop() }
})

// ---------------------------------------------------------------- VAL-R18b
const BIG = { name: 'big.md', text: `# Big document\n\n${words(10_000)}\n` }

test('VAL-R18b: switching revisions while a diff is computed shows only the diff of the selected pair', async () => {
  const app = await launch({ FAKE_CLI_SCENARIO_AUTHOR: 'rewrite' })
  try {
    const p = app.page
    // Two rewrites: original→revision 1 differs in every word (a heavy diff); revision 1→2 differs in a few words only.
    await startReview(app, { name: 'R18b-switch', maxRounds: 2, primary: BIG })
    await expectScenario(app, 'rewrite')
    await waitState(app, 'completed', 240_000)
    const side = p.getByTestId('diff-side')
    await p.getByTestId('tab-diff').click()
    await expect(side).toBeVisible({ timeout: 30_000 })
    const select = p.getByTestId('diff-rev')
    // The earlier pair's diff is the only one containing original words («alpha…» are deleted there).
    const t0 = Date.now()
    await select.selectOption('1') // starts the heavy computation
    await select.selectOption('2') // supersedes it
    await expect(side).toContainText('rw', { timeout: 4_000 })
    expect(Date.now() - t0, 'the superseded computation is abandoned: the selected pair shows without waiting for it').toBeLessThan(6_000)
    // A result of the abandoned computation must never be shown afterwards (the heavy one takes several seconds; watch 12 s).
    const end = Date.now() + 12_000
    while (Date.now() < end) {
      expect(await side.innerText(), 'the diff of the earlier pair must not appear').not.toContain('alpha1')
      await p.waitForTimeout(500)
    }
  } finally { await app.stop() }
})

test('VAL-R18b: after Cancel of a running Diff computation no diff appears', async () => {
  const app = await launch({ FAKE_CLI_SCENARIO_AUTHOR: 'rewrite' })
  try {
    const p = app.page
    // 19,500 words (just under the 20,000-word fallback guard) keep the computation running for tens of seconds.
    await startReview(app, { name: 'R18b-cancel', maxRounds: 1, primary: { name: 'huge.md', text: `# Huge document\n\n${words(19_500)}\n` } })
    await expectScenario(app, 'rewrite')
    try { await waitState(app, 'completed', 120_000) } catch (e) {
      const ids = runIds(app)
      const dump = {
        url: p.url(), strip: await strip(p).getAttribute('data-state').catch(() => null), body: (await p.locator('body').innerText().catch(() => '')).slice(0, 1500),
        runs: ids, backend: await Promise.all(ids.map((id) => ipc<{ state: string; roundsDone: number }>(p, 'run_get', { runId: id }).then((s) => `${s.state}/${s.roundsDone}`).catch((x) => String(x)))),
      }
      throw new Error(`${(e as Error).message}\nDIAG ${JSON.stringify(dump)}`)
    }
    await p.getByTestId('tab-diff').click()
    // Cancel is pressed inside the page the moment it exists (a Playwright click would wait for a stable element).
    await expect.poll(() => p.evaluate(() => {
      const b = [...document.querySelectorAll('button')].find((x) => x.textContent?.trim() === 'Cancel')
      b?.click()
      return !!b
    }), { message: 'a Cancel control is offered while the diff is computed', timeout: 5_000, intervals: [50] }).toBe(true)
    // The computation takes tens of seconds; watch 12 s for a result that was already queued.
    const end = Date.now() + 12_000
    while (Date.now() < end) {
      expect(await p.locator('[data-testid="diff-side"], [data-testid="diff-inline"]').count(), 'no diff after Cancel').toBe(0)
      await p.waitForTimeout(500)
    }
    await expect(p.getByTestId('tab-diff')).not.toHaveAttribute('data-state', 'active')
  } finally { await app.stop() }
})
