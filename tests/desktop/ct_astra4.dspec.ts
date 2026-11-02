// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// CARD AST-G (cruel tester): VAL-R47, a React update loop on the review page («Maximum update depth exceeded» about
// 1 s after /review/<id> opens), through the real debug app (real IPC, real engine, the default fake provider CLI).
// Stated because the card leaves them open:
// * every console message of type error and every pageerror is collected for 10 s after S7 opens, in two phases:
//   (A) a run started through the UI with the default fake (live, running), (B) S7 opened from the reviews list for a
//   STOPPED run (a one-round run, which stops);
// * the test fails on «Maximum update depth exceeded» and on any other React error/warning-as-error; the only messages
//   excluded are «Failed to load resource … 403» — static files served through the node_modules junction of this
//   worktree (status 403), which are not part of R47. They are excluded by status, not by URL.
import { test, expect, type Page } from '@playwright/test'
import { existsSync, mkdtempSync, readdirSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { Worker } from 'node:worker_threads'
import { launchApp, type App } from './harness'

test.use({ actionTimeout: 15_000 })
// The harness on this tree exports no BASE yet: the same default and override as the dev server / release frontend seam.
const BASE = process.env.SYMVOULIO_E2E_BASE ?? 'http://localhost:5173'

const PLAN = `# Plan\n\n${Array.from({ length: 60 }, (_, i) => `plan${i}`).join(' ')}\n`
const strip = (page: Page) => page.getByTestId('status-strip')

async function startReview(app: App, name: string, maxRounds?: number) {
  const p = app.page
  await p.getByTestId('new-review').click()
  await p.getByTestId('s6-name').fill(name)
  await p.getByTestId('s6-objective').fill('Decide whether the plan is sound.')
  await p.getByTestId('add-criterion').click()
  await p.getByTestId('criterion-0').fill('Totals must reconcile.')
  await p.getByTestId('approve-criteria').click()
  await p.getByTestId('primary-input').setInputFiles({ name: 'plan.md', mimeType: 'application/octet-stream', buffer: Buffer.from(PLAN) })
  if (maxRounds !== undefined) {
    await p.getByTestId('policy-toggle').click()
    await p.getByTestId(`s6pol-max-${maxRounds}`).click()
  }
  await p.getByTestId('start').click()
  await expect(p.locator('[data-screen="S7"]')).toBeVisible()
}

interface Seen { phase: string; at: number; kind: 'console' | 'pageerror'; text: string }
const STATIC_403 = /Failed to load resource:.*status of 403/

test('VAL-R47: the review page renders without a React update loop', async () => {
  const dataDir = mkdtempSync(join(tmpdir(), 'symvoulio-e2e-'))
  writeFileSync(join(dataDir, 'stdin.log'), '')
  const app = await launchApp({ dataDir })
  const seen: Seen[] = []
  let excluded403 = 0
  let phase = 'setup'
  const t0 = () => Date.now()
  let base = t0()
  try {
    const p = app.page
    p.on('console', (m) => {
      if (m.type() !== 'error') return
      if (STATIC_403.test(m.text())) { excluded403++; return }
      seen.push({ phase, at: Date.now() - base, kind: 'console', text: m.text() })
    })
    p.on('pageerror', (e) => seen.push({ phase, at: Date.now() - base, kind: 'pageerror', text: e.message }))

    // (A) a live review started through the UI with the default fake.
    phase = 'A: running review'
    await startReview(app, 'R47 live')
    base = t0()
    await p.waitForTimeout(10_000)
    await expect(strip(p), 'the review page is still there').toBeVisible()

    // (B) S7 of a STOPPED run, opened from the reviews list.
    await p.goto(`${BASE}/reviews`)
    await p.waitForFunction(() => '__TAURI_INTERNALS__' in window, null, { timeout: 30_000 })
    phase = 'B-setup: one-round run'
    await startReview(app, 'R47 stopped', 1)
    await expect(strip(p)).toHaveAttribute('data-state', 'completed', { timeout: 90_000 })
    await p.goto(`${BASE}/reviews`)
    await p.waitForFunction(() => '__TAURI_INTERNALS__' in window, null, { timeout: 30_000 })
    phase = 'B: stopped review opened from the list'
    await p.getByTestId('runs').locator('tr', { hasText: 'R47 stopped' }).getByTestId('open').click()
    await expect(p.locator('[data-screen="S7"]')).toBeVisible()
    await expect(strip(p)).toHaveAttribute('data-state', 'completed')
    base = t0()
    await p.waitForTimeout(10_000)
  } finally {
    await app.stop()
  }
  console.log(`R47: ${seen.length} console error/pageerror message(s); ${excluded403} static 403 message(s) excluded by status`)
  for (const s of seen) console.log(`R47 [${s.phase}] +${s.at}ms ${s.kind}: ${s.text.slice(0, 300)}`)
  const loops = seen.filter((s) => /Maximum update depth exceeded/i.test(s.text))
  expect.soft(loops, 'React «Maximum update depth exceeded» (a setState loop)').toEqual([])
  expect(seen.map((s) => `[${s.phase}] ${s.kind}: ${s.text.slice(0, 200)}`), 'no other console error or pageerror on the review page').toEqual([])
})

// ---------------------------------------------------------------- VAL-R42ui (post-commit publication failure, real IPC)
// The backend commits run_state.json and only then publishes output/final and output/audit. A fault AFTER the commit
// must reach the UI as «decision saved, publication incomplete / will be repaired» (not as a failed decision and not
// as the generic «Operation failed»), through the real commands: nothing here forges an error string.
// The fault: when the STAGED decision (<run>/.publication/output/final/human_decision.json) names the decider, the
// published output/final/human_decision.json is moved aside and replaced by a directory, so the publishing rename
// fails. Fired (L55) only if the worker reported it, the destination is a directory afterwards, and the moved-aside
// file is the OLD decision (a file already holding the new one means the publication had succeeded before the fault).
const DECIDER = 'Ruth-Decider-5521'
const prompts = (app: App): string[] =>
  readFileSync(join(app.dataDir, 'stdin.log'), 'utf8').split('\n').filter(Boolean).map((l) => JSON.parse(l) as string)
const authorPrompts = (app: App) => prompts(app).filter((x) => x.includes('<<<OPEN_FINDINGS')).length
const runDirOf = (app: App) => {
  const root = join(app.workspace, 'runs')
  return join(root, readdirSync(root).filter((n) => statSync(join(root, n)).isDirectory())[0])
}
const readJson = (file: string) => JSON.parse(readFileSync(file, 'utf8')) as any

/** Scenarios and knobs go only through this parameter list (launchApp's scenario / env). */
async function launch(env: Record<string, string> = {}, scenario?: string): Promise<App> {
  const dataDir = mkdtempSync(join(tmpdir(), 'symvoulio-e2e-'))
  writeFileSync(join(dataDir, 'stdin.log'), '')
  return launchApp({ dataDir, scenario, env: { FAKE_CLI_STDIN_LOG: join(dataDir, 'stdin.log'), ...env } })
}
function startInjector(dir: string) {
  const dest = join(dir, 'output', 'final', 'human_decision.json')
  const aside = join(dir, '..', `aside-${Date.now()}`)
  let message = 'none'
  const worker = new Worker(`
    const fs = require('fs'); const { workerData, parentPort } = require('worker_threads');
    const end = Date.now() + 40000;
    while (Date.now() < end) {
      let t = ''; try { t = fs.readFileSync(workerData.staged, 'utf8'); } catch (e) { /* no staged file yet */ }
      if (t.includes(workerData.marker)) {
        try { fs.mkdirSync(workerData.aside, { recursive: true }); fs.renameSync(workerData.dest, workerData.aside + '/human_decision.json'); fs.mkdirSync(workerData.dest); parentPort.postMessage('fired'); }
        catch (e) { parentPort.postMessage('failed: ' + e.message); }
        break;
      }
    }`, { eval: true, workerData: { staged: join(dir, '.publication', 'output', 'final', 'human_decision.json'), marker: DECIDER, dest, aside } })
  worker.on('message', (m) => { message = String(m) })
  const movedFile = join(aside, 'human_decision.json')
  return {
    async finish() {
      await new Promise((r) => setTimeout(r, 500))
      await worker.terminate()
      const fired = message === 'fired' && existsSync(dest) && statSync(dest).isDirectory() && existsSync(movedFile) && !readFileSync(movedFile, 'utf8').includes(DECIDER)
      return { fired, message }
    },
    /** Remove the obstruction and put the previous file back. */
    clear() {
      rmSync(dest, { recursive: true, force: true })
      if (existsSync(movedFile)) renameSync(movedFile, dest)
    },
  }
}
async function openS9AndFill(app: App, button: 'approve' | 'return') {
  const p = app.page
  await p.getByTestId('human-decision').click()
  await expect(p.locator('[data-screen="S9"]')).toBeVisible()
  await expect(p.getByTestId(button)).toBeEnabled()
  await p.getByTestId(button).click()
  await p.getByTestId('decider-name').fill(DECIDER)
  await p.getByTestId('decider-note').fill('Checked against the ledger')
}
/** The three files that carry the decision must agree. */
function incoherence(dir: string): string[] {
  const st = readJson(join(dir, 'run_state.json'))
  const want = JSON.stringify(st.snapshot.decision ?? null)
  const fin = readJson(join(dir, 'output', 'final', 'human_decision.json'))
  const man = readJson(join(dir, 'output', 'audit', 'run_manifest.json'))
  const bad: string[] = []
  if (JSON.stringify(fin ?? null) !== want) bad.push(`final/human_decision.json=${JSON.stringify(fin)} vs run_state.json=${want}`)
  if (JSON.stringify(man.human_decision ?? null) !== want) bad.push(`audit/run_manifest.json human_decision=${JSON.stringify(man.human_decision)} vs run_state.json=${want}`)
  if (man.state !== st.snapshot.state) bad.push(`audit/run_manifest.json state=${man.state} vs run_state.json=${st.snapshot.state}`)
  return bad
}

test('VAL-R42ui-approve: an approval whose publication fails after the state commit is shown as saved, and Export reconciles final/ and audit/', async () => {
  let app!: App
  let fired = false
  let note = ''
  try {
    for (let attempt = 1; attempt <= 3 && !fired; attempt++) {
      if (app) await app.stop()
      app = await launch()
      const p = app.page
      await startReview(app, `R42ui approve #${attempt}`, 1)
      await expect(strip(p)).toHaveAttribute('data-state', 'completed', { timeout: 90_000 })
      const dir = runDirOf(app)
      await openS9AndFill(app, 'approve')
      const inj = startInjector(dir)
      await p.getByTestId('confirm').click()
      await Promise.race([
        p.getByTestId('decided').waitFor({ timeout: 15_000 }).catch(() => {}),
        p.locator('[role="alert"]').first().waitFor({ timeout: 15_000 }).catch(() => {}),
      ])
      await p.waitForTimeout(1000)
      const r = await inj.finish()
      fired = r.fired
      note = `attempt ${attempt}: injector ${r.message}, fired=${r.fired}`
      console.log(`R42ui-approve ${note}`)
      if (!fired) { inj.clear(); continue }

      const alerts = (await p.locator('[role="alert"]').allInnerTexts()).join(' | ')
      const body = await p.locator('body').innerText()
      const decidedShown = await p.getByTestId('decided').isVisible()
      console.log(`R42ui-approve alerts: ${alerts || '(none)'}; decided card visible: ${decidedShown}`)
      const st = readJson(join(dir, 'run_state.json'))
      console.log(`R42ui-approve run_state.json: state=${st.snapshot.state}, decision=${st.snapshot.decision?.name ?? null}`)
      expect.soft(st.snapshot.decision?.name, 'run_state.json holds the approval').toBe(DECIDER)
      expect.soft(st.snapshot.state, 'run_state.json state').toBe('approved')
      expect.soft(alerts, 'no generic «Operation failed» for a decision that was committed').not.toMatch(/Operation failed/i)
      expect.soft(alerts, 'no «decision failed»').not.toMatch(/decision failed/i)
      expect.soft(body, 'the UI says the decision is SAVED').toMatch(/saved/i)
      expect.soft(body, 'the UI says the package is incomplete or will be repaired').toMatch(/incomplete|repair/i)
      expect.soft(decidedShown, 'the decided card (approved) is shown').toBe(true)

      // Remove the obstruction; Export from S10; final/ and audit/ agree with run_state.json.
      inj.clear()
      await p.goto(`${BASE}/review/${dir.split(/[\\/]/).pop()}/export`)
      await expect(p.getByTestId('do-export')).toBeVisible({ timeout: 30_000 })
      await p.getByTestId('do-export').click()
      await Promise.race([
        p.getByTestId('export-result').waitFor({ timeout: 20_000 }).catch(() => {}),
        p.locator('[role="alert"]').first().waitFor({ timeout: 20_000 }).catch(() => {}),
      ])
      expect.soft(await p.getByTestId('export-result').isVisible(), 'Export succeeded').toBe(true)
      const bad = incoherence(dir)
      console.log(`R42ui-approve after Export: ${bad.length ? bad.join('; ') : 'final/ and audit/ agree with run_state.json'}`)
      expect.soft(bad, 'after Export, final/ and audit/ agree with run_state.json').toEqual([])
    }
    expect(fired, `L55: the post-commit publication fault fired (${note})`).toBe(true)
  } finally { if (app) await app.stop() }
})

test('VAL-R42ui-return: «Return» whose publication fails after the state commit still launches the next round', async () => {
  let app!: App
  let fired = false
  let note = ''
  try {
    for (let attempt = 1; attempt <= 3 && !fired; attempt++) {
      if (app) await app.stop()
      app = await launch({ FAKE_CLI_DELAY_MS: '3000' })
      const p = app.page
      await startReview(app, `R42ui return #${attempt}`, 3)
      await expect(p.getByTestId('round-1'), 'round 1 is finished').toBeVisible({ timeout: 60_000 })
      await p.getByTestId('stop').click()
      await expect(strip(p)).toHaveAttribute('data-state', 'stopped', { timeout: 30_000 })
      const dir = runDirOf(app)
      const authorsBefore = authorPrompts(app)
      await openS9AndFill(app, 'return')
      const inj = startInjector(dir)
      await p.getByTestId('confirm').click()
      await p.waitForTimeout(4000)
      const r = await inj.finish()
      fired = r.fired
      note = `attempt ${attempt}: injector ${r.message}, fired=${r.fired}`
      console.log(`R42ui-return ${note}`)
      if (!fired) { inj.clear(); continue }

      const alerts = (await p.locator('[role="alert"]').allInnerTexts()).join(' | ')
      const st = readJson(join(dir, 'run_state.json'))
      const committed = (st.snapshot.decisionLog ?? []).some((d: any) => d.name === DECIDER && d.kind === 'returned')
      console.log(`R42ui-return alerts: ${alerts || '(none)'}; Return committed in run_state.json: ${committed}; author prompts before ${authorsBefore}, now ${authorPrompts(app)}`)
      expect.soft(committed, 'run_state.json holds the Return decision').toBe(true)
      expect.soft(alerts, 'no generic «Operation failed» for a decision that was committed').not.toMatch(/Operation failed/i)
      await expect.soft(strip(p), 'the status strip reaches running').toHaveAttribute('data-state', 'running', { timeout: 30_000 })
      await expect.poll(() => authorPrompts(app), { message: 'a NEW author prompt appears in FAKE_CLI_STDIN_LOG', timeout: 30_000 }).toBeGreaterThan(authorsBefore)
      inj.clear()
    }
    expect(fired, `L55: the post-commit publication fault fired (${note})`).toBe(true)
  } finally { if (app) await app.stop() }
})
