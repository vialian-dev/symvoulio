// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// CARD AST-D: the external review's pass-3 desktop failures (R32 to R40), reproduced through the real debug app (real IPC, real engine,
// the fake provider CLI) over WebView2 CDP. One test per VAL; each test name starts with its VAL id.
// Opt-in fake-CLI knobs used here (crates/symvoulio-core/src/bin/fake-cli.rs): FAKE_CLI_STDIN_LOG, FAKE_CLI_SCENARIO_AUTHOR,
// FAKE_CLI_DELAY_MS, FAKE_CLI_LIVE_LINES, scenarios `human_question` and `live_error`, and the knob this card adds:
// FAKE_CLI_QUESTIONS_BY_ROUND (a JSON array: the question text of round n, id R01-099 in every round).
// Backend commands are called only to set up or read a precondition (run_get, shell_info, settings_get/settings_set);
// the action under test is always driven through the UI. No page reload is used to refresh.
import { test, expect, type Page } from '@playwright/test'
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { Worker } from 'node:worker_threads'
import { launchApp, relaunch, ipc, scenariosRun, type App } from './harness'

test.use({ actionTimeout: 15_000 })
// The harness on this tree exports no BASE yet: the same default and override as the dev server / release frontend seam.
const BASE = process.env.SYMVOULIO_E2E_BASE ?? 'http://localhost:5173'

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
const readJson = (file: string) => JSON.parse(readFileSync(file, 'utf8')) as any

const words = (n: number, prefix = 'alpha') => Array.from({ length: n }, (_, i) => `${prefix}${i}`).join(' ')
const PLAN = `# Plan\n\n${words(60, 'plan')}\n`

interface StartOpts {
  name?: string
  primary?: { name: string; text: string | Buffer }
  supporting?: { name: string; text: string | Buffer }[]
  maxRounds?: number
  /** Do not wait for S7 (the start is expected to fail). */
  noWait?: boolean
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
  if (o.maxRounds !== undefined) {
    await p.getByTestId('policy-toggle').click()
    await p.getByTestId(`s6pol-max-${o.maxRounds}`).click()
  }
  await p.getByTestId('start').click()
  if (!o.noWait) await expect(p.locator('[data-screen="S7"]')).toBeVisible()
}
const waitState = (app: App, state: string, timeout = 90_000) => expect(strip(app.page)).toHaveAttribute('data-state', state, { timeout })
const waitPrompts = (app: App, n: number, timeout = 30_000) => expect.poll(() => prompts(app).length, { timeout }).toBeGreaterThanOrEqual(n)
/** L55: the named fake scenario (for a role override: that role's scenario name) really ran for at least one provider call. */
const expectScenario = (app: App, name: string) => expect.poll(() => scenariosRun(app), { message: `L55: the scenario «${name}» ran at least once`, timeout: 60_000 }).toContain(name)

/** Every object in a JSON tree that carries a `question` and an `issue_id` (the persisted question records). */
function questionRecords(node: unknown, out: any[] = []): any[] {
  if (Array.isArray(node)) node.forEach((n) => questionRecords(n, out))
  else if (node && typeof node === 'object') {
    const o = node as Record<string, unknown>
    if (typeof o.question === 'string' && typeof o.issue_id === 'string') out.push(o)
    Object.values(o).forEach((v) => questionRecords(v, out))
  }
  return out
}
/** The «Answers from the human» block of a prompt. */
const answersSection = (prompt: string) => prompt.split('Answers from the human:')[1] ?? ''
/** The human-questions blocks of one question each. */
const questionBlock = (page: Page, text: string) => page.getByTestId('human-questions').first().locator('div.rounded-lg').filter({ hasText: text })

// ---------------------------------------------------------------- VAL-R32
const APPROVAL_CLAIM = /\bapproved\b/i
/** Text files under output/ that claim an approval (run_manifest.json, timeline.json, final_review.html, human_decision.json). */
const approvalClaims = (dir: string): string[] =>
  filesUnder(join(dir, 'output')).filter((f) => !/\.(docx|zip)$/i.test(f) && APPROVAL_CLAIM.test(readFileSync(f, 'utf8'))).map((f) => f.slice(dir.length + 1).replace(/\\/g, '/'))

test('VAL-R32: an approval whose state commit fails is claimed by no file under output/ (also after the app is relaunched)', async () => {
  let injected = false
  let trigger = ''
  let sawError = false
  let app!: App
  let current!: App
  // The state commit must fail on the SECOND save of the decision (the first save refreshes the committed state, and a
  // failure there leaves nothing behind). A worker thread polls human_decision.json in a tight loop and, the moment it
  // says Approved, takes the temporary name of the state file with a directory: the commit that follows fails.
  // Up to 3 attempts (a fresh run each) in case the injection lands after the commit; the result is judged on the one that landed.
  for (let attempt = 1; attempt <= 3 && !(injected && sawError); attempt++) {
    if (app) await current.stop().catch(() => {})
    app = await launch()
    current = app
    injected = false
    sawError = false
    const p = app.page
    await startReview(app, { name: `R32 #${attempt}`, maxRounds: 1 })
    await waitState(app, 'completed')
    const dir = runDir(app)
    expect(approvalClaims(dir), 'precondition: a stopped, undecided run has no file that claims an approval').toEqual([])
    await p.getByTestId('human-decision').click()
    await expect(p.locator('[data-screen="S9"]')).toBeVisible()
    await p.getByTestId('approve').click()
    await p.getByTestId('decider-name').fill('Dana Reviewer')
    await p.getByTestId('decider-note').fill('Checked against the ledger')
    const file = join(dir, 'output', 'final', 'human_decision.json')
    const tmp = join(dir, 'run_state.json.tmp')
    // The engine stages every artifact in <run>/.publication/, commits run_state.json and only then publishes: the trigger
    // is the STAGED decision (any file under .publication/ that says approved), which exists before the commit. The
    // published file stays a second trigger for an engine that writes output/ directly (it has no staging directory).
    const staging = join(dir, '.publication')
    const worker = new Worker(`
      const fs = require('fs'); const path = require('path'); const { workerData, parentPort } = require('worker_threads');
      const walk = (d) => fs.readdirSync(d, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(path.join(d, e.name)) : [path.join(d, e.name)]));
      const claims = (f) => { try { return /approved/i.test(fs.readFileSync(f, 'utf8')); } catch (e) { return false; } };
      const end = Date.now() + 40000;
      while (Date.now() < end) {
        let staged = false;
        try { staged = walk(workerData.staging).some(claims); } catch (e) { /* no staging directory right now */ }
        if (staged || claims(workerData.file)) { fs.mkdirSync(workerData.tmp); parentPort.postMessage(staged ? 'staged' : 'published'); break; }
      }`, { eval: true, workerData: { file, tmp, staging } })
    worker.on('message', (m) => { injected = true; trigger = String(m) })
    await p.getByTestId('confirm').click()
    sawError = await p.locator('[data-screen="S9"] [role="alert"], [data-testid="confirm-dialog"] [role="alert"]').first().waitFor({ state: 'visible', timeout: 15_000 }).then(() => true, () => false)
    await new Promise((r) => setTimeout(r, 500)) // lets the worker's message arrive
    await worker.terminate()
    if (!(injected && sawError)) continue

    // The UI did not accept the approval ...
    await expect(p.getByTestId('decided')).toHaveCount(0)
    // ... the committed state does not hold it ...
    const st = readJson(join(dir, 'run_state.json'))
    expect(st.snapshot.state, 'run_state.json keeps the pre-approval state').not.toBe('approved')
    expect(st.snapshot.decision ?? null, 'run_state.json holds no decision').toBeNull()
    // ... and therefore no file under output/ may claim it.
    const hd = JSON.parse(readFileSync(file, 'utf8'))
    expect.soft(hd?.kind ?? null, 'output/final/human_decision.json must not say Approved').not.toBe('approved')
    const manifest = readJson(join(dir, 'output', 'audit', 'run_manifest.json'))
    expect.soft(manifest.human_decision ?? null, 'the manifest must not bind the uncommitted decision').toBeNull()
    expect.soft(manifest.state, 'the manifest state is the committed one').not.toBe('approved')
    expect.soft(approvalClaims(dir), 'no file under output/ claims an approval run_state.json does not hold').toEqual([])

    // After a relaunch the disagreement must not be there either (and not be "repaired" into an approval).
    rmSync(tmp, { recursive: true, force: true })
    current = await relaunch(app)
    const after = readJson(join(dir, 'run_state.json'))
    expect(after.snapshot.state).not.toBe('approved')
    expect.soft(approvalClaims(dir), 'after the relaunch: no file under output/ claims an approval').toEqual([])
    expect.soft(JSON.parse(readFileSync(file, 'utf8'))?.kind ?? null, 'after the relaunch: human_decision.json is not Approved').not.toBe('approved')
  }
  try {
    console.log(`R32 injector trigger: ${trigger || 'none'}`)
    expect(injected, 'the state commit failure was injected right after the decision was written (the test is not vacuous)').toBe(true)
    expect(sawError, 'the UI shows the failed approval as an error').toBe(true)
  } finally { await current.stop() }
})

// ---------------------------------------------------------------- VAL-R33a
test('VAL-R33a: «Return» on a stopped run moves the UI to Running and shows the new round, it does not stay Stopped', async () => {
  const app = await launch({ FAKE_CLI_DELAY_MS: '3000' })
  try {
    const p = app.page
    await startReview(app, { name: 'R33a', maxRounds: 3 })
    await expect(p.getByTestId('round-1'), 'round 1 is finished').toBeVisible({ timeout: 60_000 })
    await p.getByTestId('stop').click()
    await waitState(app, 'stopped', 30_000)
    const id = runIds(app)[0]
    const done = (await ipc<{ roundsDone: number }>(p, 'run_get', { runId: id })).roundsDone // precondition read
    await p.getByTestId('human-decision').click()
    await expect(p.locator('[data-screen="S9"]')).toBeVisible()
    await expect(p.getByTestId('return'), 'Return is offered below the maximum of rounds').toBeEnabled()
    await p.getByTestId('return').click()
    await p.getByTestId('decider-name').fill('Dana Reviewer')
    await p.getByTestId('confirm').click()
    await expect(p.locator('[data-screen="S7"]'), 'Return opens the workspace').toBeVisible({ timeout: 15_000 })
    // The provider calls take 3 s each, so a returned run stays running for several seconds: Running must show.
    const seen: string[] = []
    const t0 = Date.now()
    let sawRunning = false
    while (Date.now() - t0 < 20_000 && !sawRunning) {
      const s = (await strip(p).getAttribute('data-state').catch(() => null)) ?? '?'
      if (seen.at(-1)?.split('@')[0] !== s) seen.push(`${s}@${Date.now() - t0}`)
      sawRunning = s === 'running'
      if (!sawRunning) await p.waitForTimeout(100)
    }
    expect(sawRunning, `the UI moves to Running after Return (status strip: ${seen.join(' ')})`).toBe(true)
    await expect(strip(p), 'the status strip shows the new round').toContainText(`Round ${done + 1} of 3`)
    await expect(p.getByTestId('round-live')).toBeVisible()
    // And it stays running while the round is under way (it must not fall back to Stopped).
    await p.waitForTimeout(1500)
    await expect(strip(p)).toHaveAttribute('data-state', 'running')
  } finally { await app.stop() }
})

// ---------------------------------------------------------------- VAL-R33b
test('VAL-R33b: after a storage failure shows Paused, a delayed Running result of an earlier read does not bring Running back', async () => {
  const DELAY = 15_000
  const app = await launch({ FAKE_CLI_DELAY_MS: '5000' })
  try {
    const p = app.page
    await startReview(app, { name: 'R33b' })
    // Timing: while armed, the RESPONSE of every run_get (the real backend answer, read at the moment of the call) is held
    // back for 15 s at the network layer (CDP Fetch on the IPC channel; Tauri locks window.__TAURI_INTERNALS__.invoke, so
    // it cannot be wrapped in the page). The call that begins next fires its phase refresh, which reads Running and is delayed.
    const cdp = await p.context().newCDPSession(p)
    let armed = true
    let held = 0
    cdp.on('Fetch.requestPaused', (e: any) => {
      const release = () => void cdp.send('Fetch.continueResponse', { requestId: e.requestId }).catch(() => {})
      // The CORS preflight (OPTIONS) goes through at once; only the answer to the real call is held.
      if (!armed || e.request.method === 'OPTIONS') { release(); return }
      held++
      setTimeout(release, DELAY)
    })
    await cdp.send('Fetch.enable', { patterns: [{ urlPattern: 'http://ipc.localhost/run_get*', requestStage: 'Response' }] })
    await waitPrompts(app, 2) // the second provider call has begun: its phase refresh has read Running
    await expect.poll(() => held, { message: 'a Running read is in flight (held back)', timeout: 5_000 }).toBeGreaterThanOrEqual(1)
    armed = false
    // The state of the run becomes unreadable and unwritable, as in VAL-R29b.
    const state = join(runDir(app), 'run_state.json')
    rmSync(state)
    mkdirSync(state)
    // Watch the status strip every 100 ms. The long observation (about 40 s) is the timing under test: the held-back
    // answer lands 15 s after it was asked, i.e. about 10 s after Paused shows.
    const timeline: string[] = []
    let pausedAt = -1
    const t0 = Date.now()
    let violation = ''
    while (Date.now() - t0 < 45_000) {
      const s = (await strip(p).getAttribute('data-state').catch(() => null)) ?? '?'
      const t = Date.now() - t0
      if (timeline.at(-1)?.split('@')[0] !== s) timeline.push(`${s}@${t}`)
      if (s === 'paused' && pausedAt < 0) pausedAt = t
      if (pausedAt >= 0 && s === 'running' && !violation) violation = `running@${t} after paused@${pausedAt}`
      if (pausedAt >= 0 && t - pausedAt > 25_000) break
      await p.waitForTimeout(100)
    }
    expect(pausedAt, `precondition: the storage failure shows Paused (timeline ${timeline.join(' ')})`).toBeGreaterThanOrEqual(0)
    expect(violation, `Running came back after Paused (timeline ${timeline.join(' ')})`).toBe('')
    await expect(strip(p)).not.toHaveAttribute('data-state', 'running')
  } finally { await app.stop() }
})

// ---------------------------------------------------------------- VAL-R34
const Q1 = 'Who owns the capital budget?'
const nearby = (text: string, a: string, b: string, within: number) => {
  for (let i = text.indexOf(a); i >= 0; i = text.indexOf(a, i + 1)) {
    const lo = Math.max(0, i - within), hi = i + a.length + within
    if (text.slice(lo, hi).includes(b)) return true
  }
  return false
}

test('VAL-R34: the next author prompt and the exported report carry the QUESTION with its answer, and the answer carries the answerer entered in the UI', async () => {
  const NAME = 'Dana Answerer'
  const app = await launch({ FAKE_CLI_SCENARIO_AUTHOR: 'human_question', FAKE_CLI_DELAY_MS: '1500' })
  try {
    const p = app.page
    await startReview(app, { name: 'R34', maxRounds: 2 })
    await expectScenario(app, 'human_question')
    await waitPrompts(app, 1)
    await p.getByTestId('pause').click()
    await waitState(app, 'paused', 30_000)
    const block = questionBlock(p, Q1)
    await expect(block).toBeVisible()
    // The UI must collect who answers: one «Answered by» field above the questions section (a sibling outside it),
    // filled before answering; the section and each block keep exactly one textbox.
    const nameField = p.getByLabel(/answered by/i)
    const offered = await nameField.count()
    expect.soft(offered, 'the page has exactly one «Answered by» field').toBe(1)
    if (offered === 1) await nameField.fill(NAME)
    await block.getByRole('textbox').fill('Finance')
    await block.getByRole('button').first().click()
    await expect(block).toContainText('Finance')

    // The next author prompt (the run is resumed) carries the question with its answer.
    const before = prompts(app).length
    await p.getByTestId('resume').click()
    await expect.poll(() => prompts(app).slice(before).some(isAuthorPrompt), { timeout: 60_000 }).toBe(true)
    const next = prompts(app).slice(before).find(isAuthorPrompt)!
    const section = answersSection(next)
    expect.soft(section, 'the answers block of the next author prompt holds the question text').toContain(Q1)
    expect.soft(nearby(section, Q1, 'Finance', 300), 'the question text stands together with its answer').toBe(true)

    // Export through S10, then final_review.html.
    await waitState(app, 'completed', 90_000)
    await p.getByTestId('export').click()
    await expect(p.locator('[data-screen="S10"]')).toBeVisible()
    await p.getByTestId('do-export').click()
    await expect(p.getByTestId('export-result')).toBeVisible()
    const html = readFileSync(join(runDir(app), 'output', 'final', 'final_review.html'), 'utf8')
    expect.soft(html, 'the report shows the question').toContain(Q1)
    expect.soft(nearby(html, Q1, 'Finance', 600), 'the report shows the question with its answer').toBe(true)
    expect.soft(html, 'the report names the answering person entered in the UI').toContain(NAME)
    expect.soft(nearby(html, 'Finance', NAME, 600), 'the answerer stands with the answer').toBe(true)
  } finally { await app.stop() }
})

// ---------------------------------------------------------------- VAL-R35
test('VAL-R35: a second question under the same id in a later round does not change the first answer (UI, run_state.json, next prompt)', async () => {
  const QA = 'Who owns the capital budget?'
  const QB = 'Which team signs off the vendor contract?'
  const app = await launch({
    FAKE_CLI_SCENARIO_AUTHOR: 'human_question', FAKE_CLI_DELAY_MS: '1500',
    FAKE_CLI_QUESTIONS_BY_ROUND: JSON.stringify([QA, QB]),
  })
  try {
    const p = app.page
    await startReview(app, { name: 'R35', maxRounds: 3 })
    await expectScenario(app, 'human_question')
    await waitPrompts(app, 1)
    await p.getByTestId('pause').click()
    await waitState(app, 'paused', 30_000)
    const a = questionBlock(p, QA)
    await expect(a).toBeVisible()
    await a.getByRole('textbox').fill('Finance')
    await a.getByRole('button').first().click()
    await expect(a).toContainText('Finance')

    // Round 2: the author asks QB under the same id. Two compliant outcomes (CODE_REVIEW_ASTRA3 R35 fix):
    //  rule 1: the round is rejected (nothing written): QA with Finance is all there is;
    //  rule 2: QB is kept under a DISTINCT id and answering one leaves the other unchanged.
    const before = prompts(app).length
    await p.getByTestId('resume').click()
    await expect.poll(() => prompts(app).slice(before).some((q) => isAuthorPrompt(q) && q.includes('ROUND: 2')), { timeout: 60_000 }).toBe(true)
    // A rejected round may already have paused the run (the Pause control is then replaced by Resume).
    if ((await p.getByTestId('pause').count()) > 0 && (await p.getByTestId('pause').isEnabled())) await p.getByTestId('pause').click().catch(() => {})
    await waitState(app, 'paused', 30_000)
    const stateFile = () => readJson(join(runDir(app), 'run_state.json')).ledger
    const recs = () => questionRecords(stateFile())
    const byText = (t: string) => recs().filter((r) => r.question === t)
    const idA = byText(QA)[0]?.issue_id as string
    expect(idA, 'precondition: the first question is recorded').toBeTruthy()
    const rejected = byText(QB).length === 0
    const answerB = async () => {
      const b = questionBlock(p, QB)
      await expect(b, 'the second question is shown').toBeVisible()
      await b.getByRole('textbox').fill('Security')
      await b.getByRole('button').first().click()
      await expect(b).toContainText('Security')
    }
    if (rejected) {
      // rule 1: visible rejection (a visible error message that names the problem; Paused alone does not count),
      // the second question nowhere, the first answer unchanged.
      const alerts = (await p.locator('[role="alert"]:visible').allInnerTexts()).join('\n')
      expect(alerts, 'rule 1: the reused-id round is rejected with a visible error that names the question, its id or the rejection').toMatch(/question|R\d\d-\d{3}|reject|duplicate|already/i)
      await expect(questionBlock(p, QB), 'rule 1: the rejected question is not shown').toHaveCount(0)
      expect.soft(recs().map((r) => `${r.issue_id}|${r.question}|${r.answer}`), 'rule 1: run_state.json holds the first question with Finance, nothing else').toEqual([`${idA}|${QA}|Finance`])
    } else {
      // rule 2: distinct ids, then answering the second leaves the first unchanged.
      const idB = byText(QB)[0].issue_id as string
      expect.soft(idB, 'rule 2: the second question has an id of its own (it reuses the first id: neither rule is met)').not.toBe(idA)
      expect.soft(recs().filter((r) => r.issue_id === idA).length, 'rule 2: the first id belongs to one question only').toBe(1)
      await answerB()
      expect.soft(byText(QB).map((r) => r.answer), 'rule 2: run_state.json: the second record holds Security').toEqual(['Security'])
      expect.soft(recs().length, 'rule 2: two question records are kept').toBe(2)
      await expect.soft(questionBlock(p, QB), 'rule 2: UI: the second block shows Security').toContainText('Security')
    }
    // Both rules: the first answer is unchanged in the UI and in run_state.json.
    await expect.soft(questionBlock(p, QA), 'UI: the first answer is unchanged').toContainText('Finance')
    await expect.soft(questionBlock(p, QA), 'UI: the first question does not show the second answer').not.toContainText('Security')
    expect.soft(byText(QA).map((r) => r.answer), 'run_state.json: the first record keeps Finance').toEqual(['Finance'])

    // The next author prompt: Finance once and with its question; Security (rule 2) once and with its question, never on rule 1.
    const before2 = prompts(app).length
    await p.getByTestId('resume').click()
    await expect.poll(() => prompts(app).slice(before2).some(isAuthorPrompt), { timeout: 60_000 }).toBe(true)
    const section = answersSection(prompts(app).slice(before2).find(isAuthorPrompt)!)
    expect.soft(section.split('Finance').length - 1, 'prompt: the first answer Finance is delivered once').toBe(1)
    expect.soft(nearby(section, QA, 'Finance', 300), 'prompt: Finance stands with the first question').toBe(true)
    expect.soft(section.split('Security').length - 1, rejected ? 'prompt: no second answer exists' : 'prompt: the second answer Security is delivered once').toBe(rejected ? 0 : 1)
    if (!rejected) expect.soft(nearby(section, QB, 'Security', 300), 'prompt: Security stands with the second question').toBe(true)
  } finally { await app.stop() }
})

// ---------------------------------------------------------------- VAL-R36
const R36_LINES = [
  'LIVEPROBE-36 reviewing section 3 of 7: capital budget',
  '{"token":"shortSecret"}',
  'password: hunter2',
  'secret=s3cr3tValue',
  '"client_secret": "cs_live_abc"',
  'X-Api-Key: k_12345',
]
const R36_VALUES = ['shortSecret', 'hunter2', 's3cr3tValue', 'cs_live_abc', 'k_12345']
/** Every file under the data dir except the browser profile and the test's own logs (stdin.log, fake-cli.log). */
const appFiles = (app: App): string[] => filesUnder(app.dataDir).filter((f) => {
  const rel = f.slice(app.dataDir.length + 1).replace(/\\/g, '/')
  return !rel.startsWith('webview2/') && rel !== 'stdin.log' && rel !== 'fake-cli.log' && statSync(f).size < 50_000_000
})
const leaks36 = (text: string) => R36_VALUES.filter((v) => text.includes(v) || text.includes(JSON.stringify(v).slice(1, -1)))
const SUMMARY = /assistant text, \d+ words?/i
/** The raw toggle «Show raw provider output (not credential-safe)» behind its fold. The fold and the toggle have no
 *  test ids in the decision, so they are found by their visible text: the fold opener is the first button, summary or
 *  role=button whose text names the raw output; the toggle is the control labelled «Show raw provider output». */
const rawToggle = (p: Page) => p.getByLabel(/Show raw provider output/i)
async function openRawFold(p: Page) {
  if (await rawToggle(p).isVisible()) return
  await p.locator('summary, button, [role="button"]').filter({ hasText: /raw provider output/i }).first().click()
  await expect(rawToggle(p), 'the raw toggle is behind a fold').toBeVisible()
}
async function rawOn(p: Page) {
  await openRawFold(p)
  await rawToggle(p).check()
}
/** Samples the body text while a run goes on; the run's later calls re-emit the live lines, so a toggle switched on
 *  just after the start still sees them. Stops at `until` or after `ms`. */
async function sampleBody(app: App, until: (s: string | null) => boolean, ms = 60_000) {
  let seen = ''
  const end = Date.now() + ms
  while (Date.now() < end) {
    seen += '\n' + (await app.page.locator('body').innerText().catch(() => ''))
    if (until(await strip(app.page).getAttribute('data-state').catch(() => null))) break
    await app.page.waitForTimeout(200)
  }
  return seen + '\n' + (await app.page.locator('body').innerText().catch(() => ''))
}
const filesHits = (app: App, needles: string[]) => {
  const hits: string[] = []
  for (const f of appFiles(app)) {
    const t = readFileSync(f, 'utf8')
    const found = needles.filter((n) => t.includes(n) || t.includes(JSON.stringify(n).slice(1, -1)))
    if (found.length) hits.push(`${f.slice(app.dataDir.length + 1)}: ${found.join(',')}`)
  }
  return hits
}

test('VAL-R36: credential shapes the redactor does not list in live provider output reach neither the UI, run_state.json, output/ nor the app files (success and error path)', async () => {
  for (const path of ['success', 'error'] as const) {
    const app = await launch({ FAKE_CLI_LIVE_LINES: JSON.stringify(R36_LINES), FAKE_CLI_DELAY_MS: '3000' }, path === 'error' ? 'live_error' : undefined)
    try {
      const p = app.page
      await startReview(app, { name: `R36 ${path}`, maxRounds: 1 })
      if (path === 'error') await expectScenario(app, 'live_error')
      let seen = ''
      const sample = async () => { seen += '\n' + (await p.locator('body').innerText().catch(() => '')) }
      // The DEFAULT panel: the raw toggle is never touched (the decision of 2026-10-31 05:52Z, option A).
      const end = Date.now() + 90_000
      while (Date.now() < end) {
        await sample()
        const s = await strip(p).getAttribute('data-state')
        if (path === 'error' ? s === 'paused' : s === 'stopped' || s === 'completed') break
        await p.waitForTimeout(200)
      }
      await sample()
      expect.soft(seen, `${path}: a live event still appears in the panel as «assistant text, N words» (the panel moves)`).toMatch(SUMMARY)
      expect.soft(seen, `${path}: the default panel shows no provider text (the probe line must not appear)`).not.toContain('LIVEPROBE-36')
      expect.soft(leaks36(seen), `${path}: secret values in the UI text`).toEqual([])
      await expect.poll(() => readFileSync(join(runDir(app), 'run_state.json'), 'utf8').length, { timeout: 5_000 }).toBeGreaterThan(0)
      const hits: string[] = []
      for (const f of appFiles(app)) {
        const found = leaks36(readFileSync(f, 'utf8'))
        if (found.length) hits.push(`${f.slice(app.dataDir.length + 1)}: ${found.join(',')}`)
      }
      expect.soft(hits, `${path}: secret values in files (run_state.json, output/, logs)`).toEqual([])
    } finally { await app.stop() }
  }
})

test('VAL-R02stream-raw: with raw ON (fold opened, toggle switched) the real probe line appears in the panel while the call runs', async () => {
  const LINE = 'LIVEPROBE-36 reviewing section 3 of 7: capital budget'
  const app = await launch({ FAKE_CLI_LIVE_LINES: JSON.stringify([LINE]), FAKE_CLI_DELAY_MS: '8000' })
  try {
    const p = app.page
    await startReview(app, { name: 'R02stream-raw' })
    await rawOn(p)
    await expect.poll(async () => (await p.locator('body').innerText()).includes(LINE), { message: 'the probe line is in the panel', timeout: 40_000, intervals: [250] }).toBe(true)
    await expect(strip(p), 'it arrived while a call was running').toHaveAttribute('data-state', 'running')
  } finally { await app.stop() }
})

test('VAL-R36-reset: after an app relaunch the raw toggle is off again and the probe line no longer appears', async () => {
  const LINE = 'LIVEPROBE-36 reviewing section 3 of 7: capital budget'
  const app = await launch({ FAKE_CLI_LIVE_LINES: JSON.stringify([LINE]), FAKE_CLI_DELAY_MS: '4000' })
  let current = app
  try {
    await startReview(app, { name: 'R36 reset 1' })
    await rawOn(app.page)
    await expect.poll(async () => (await app.page.locator('body').innerText()).includes(LINE), { message: 'precondition: with raw ON the probe line appears', timeout: 40_000, intervals: [250] }).toBe(true)
    current = await relaunch(app)
    // relaunch() leaves the first-run wizard up: open the app as launchApp does for a fresh start.
    await current.page.evaluate(() => localStorage.setItem('symvoulio.setup', JSON.stringify({ setupDone: true })))
    await current.page.goto(`${BASE}/reviews`)
    await current.page.waitForFunction(() => '__TAURI_INTERNALS__' in window, null, { timeout: 30_000 })
    await startReview(current, { name: 'R36 reset 2' })
    await openRawFold(current.page)
    await expect(rawToggle(current.page), 'the toggle is off after the relaunch').not.toBeChecked()
    const seen = await sampleBody(current, () => false, 15_000)
    expect.soft(seen, 'after the relaunch the panel still moves (controlled summary)').toMatch(SUMMARY)
    expect.soft(seen, 'after the relaunch the probe line no longer appears').not.toContain('LIVEPROBE-36')
  } finally { await current.stop() }
})

test('VAL-R36-nopersist: with raw ON, no secret value and not the probe line is found in output/, the logs or run_state.json', async () => {
  const app = await launch({ FAKE_CLI_LIVE_LINES: JSON.stringify(R36_LINES), FAKE_CLI_DELAY_MS: '3000' })
  try {
    const p = app.page
    await startReview(app, { name: 'R36 nopersist', maxRounds: 1 })
    await rawOn(p)
    const seen = await sampleBody(app, (s) => s === 'stopped' || s === 'completed', 90_000)
    expect(seen, 'precondition: with raw ON the probe line appeared in the UI').toContain('LIVEPROBE-36')
    await expect.poll(() => readFileSync(join(runDir(app), 'run_state.json'), 'utf8').length, { timeout: 5_000 }).toBeGreaterThan(0)
    expect.soft(filesHits(app, [...R36_VALUES, 'LIVEPROBE-36']), 'secret values or the probe line in files (run_state.json, output/, logs)').toEqual([])
  } finally { await app.stop() }
})

// ---------------------------------------------------------------- VAL-R38
test('VAL-R38: two supporting files named notes.md fail the start visibly and leave no copy of any of the three files under the workspace, also after deleting the draft', async () => {
  const M = { primary: 'PRIMARY-MARK-r38-7f3a', a: 'NOTES-A-MARK-r38-c19d', b: 'NOTES-B-MARK-r38-52e8' }
  const marks = Object.values(M)
  const copies = (app: App) => filesUnder(app.workspace)
    .filter((f) => { try { const t = readFileSync(f, 'latin1'); return marks.some((m) => t.includes(m)) } catch { return false } })
    .map((f) => f.slice(app.workspace.length + 1).replace(/\\/g, '/'))
  const app = await launch()
  try {
    const p = app.page
    await startReview(app, {
      name: 'R38', noWait: true,
      primary: { name: 'plan.md', text: `# Plan\n\n${M.primary} ${words(60, 'plan')}\n` },
      supporting: [{ name: 'notes.md', text: `# Notes A\n\n${M.a} budget ceiling is 12 units.\n` }, { name: 'notes.md', text: `# Notes B\n\n${M.b} the other folder's notes.\n` }],
    })
    await expect(p.locator('[role="alert"]:visible').first(), 'the start failure is visible').toBeVisible({ timeout: 30_000 })
    // Soft, so that the Delete step below is exercised even when the first check is red.
    await expect.configure({ soft: true }).poll(() => copies(app), { message: 'no copy of the primary or either notes.md under the workspace after the failed start', timeout: 5_000 }).toEqual([])
    await p.getByRole('button', { name: 'Delete', exact: true }).click()
    await expect(p.locator('[data-screen="S11"]')).toBeVisible()
    await expect.configure({ soft: true }).poll(() => copies(app), { message: 'no copy after deleting the failed draft', timeout: 5_000 }).toEqual([])
  } finally { await app.stop() }
})

// ---------------------------------------------------------------- VAL-R40
test('VAL-R40: S12 lists exactly the shells shell_info reports, and a saved shell the backend reports is shown as saved after a relaunch', async () => {
  const app0 = await launch()
  let current = app0
  try {
    const info = await ipc<{ available: string[]; chosen: string }>(app0.page, 'shell_info') // precondition read
    expect(info.available.length, 'the backend reports at least one shell').toBeGreaterThan(0)
    const listed = async (app: App, panel: string) => {
      await app.page.getByTestId('nav-settings').click()
      await expect(app.page.locator('[data-screen="S12"]')).toBeVisible()
      const sel = app.page.getByTestId(`shell-${panel}`)
      await expect(sel).toBeVisible()
      return sel.locator('option').evaluateAll((os) => os.map((o) => (o as HTMLOptionElement).value))
    }
    for (const panel of ['claude', 'codex']) {
      const values = await listed(current, panel)
      expect.soft([...values].sort(), `S12 ${panel}: the options are exactly the shells the backend reports (${info.available.join(', ')})`).toEqual([...info.available].sort())
    }
    for (const shell of info.available) {
      const settings = await ipc<any>(current.page, 'settings_get') // precondition: the saved shell is set through IPC
      await ipc(current.page, 'settings_set', { settings: { ...settings, shell: { claude: shell, codex: shell } } })
      current = await relaunch(current)
      // relaunch() leaves the app on the first-run wizard (its origin has no setup flag): mark the setup done and open the
      // app, exactly as launchApp does for a fresh start. This is the app start, not a refresh of data under test.
      await current.page.evaluate(() => localStorage.setItem('symvoulio.setup', JSON.stringify({ setupDone: true })))
      await current.page.goto(`${BASE}/reviews`)
      await current.page.waitForFunction(() => '__TAURI_INTERNALS__' in window, null, { timeout: 30_000 })
      for (const panel of ['claude', 'codex']) {
        await listed(current, panel)
        await expect.soft(current.page.getByTestId(`shell-${panel}`), `S12 ${panel}: the saved shell ${shell} is shown as saved`).toHaveValue(shell)
      }
    }
    // A backend that reports ONLY Windows PowerShell (the PATH of the app lacks pwsh): S12 must not offer pwsh, and the
    // saved shell is shown as saved. This is the Windows stand-in for a platform with other shells (macOS: zsh).
    const pathKey = Object.keys(process.env).find((k) => /^path$/i.test(k)) ?? 'PATH'
    const winPs = 'C:\\Windows\\System32\\WindowsPowerShell\\v1.0'
    const narrow = await launch({ [pathKey]: `${winPs};C:\\Windows\\System32` })
    try {
      const only = await ipc<{ available: string[] }>(narrow.page, 'shell_info')
      expect(only.available, 'precondition: the narrowed PATH leaves Windows PowerShell as the only shell').toEqual(['powershell'])
      for (const panel of ['claude', 'codex']) {
        const values = await listed(narrow, panel)
        expect.soft(values, `S12 ${panel}: only the reported shell is offered (reported: ${only.available.join(', ')})`).toEqual(['powershell'])
      }
    } finally { await narrow.stop() }
  } finally { await current.stop() }
})
