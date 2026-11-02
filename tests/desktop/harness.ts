// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Harness for desktop E2E tests: the real debug app with real IPC and engine, the fake provider CLI, an isolated data
// directory (never the installed app's data) and WebView2 CDP for driving the UI.
//
//   const app = await launchApp({ scenario: 'slow' })    // fresh data dir, fake CLI scenario
//   await app.page.getByTestId(...)                      // drive the real UI
//   await ipc(app.page, 'run_get', { runId })            // call a backend command directly
//   const again = await relaunch(app, { scenario: 'ok' }) // restart the app on the SAME data dir
//   await app.stop()
import { chromium, test, type Browser, type Page } from '@playwright/test'
import { spawn, execFileSync, type ChildProcess } from 'node:child_process'
import { mkdtempSync, mkdirSync, existsSync, writeFileSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

const ROOT = resolve('.')
/** The debug app and the fake CLI; override with SYMVOULIO_APP_EXE / SYMVOULIO_FAKE_EXE. */
export const APP_EXE = process.env.SYMVOULIO_APP_EXE ?? join(ROOT, 'target', 'debug', 'symvoulio.exe')
export const FAKE_EXE = process.env.SYMVOULIO_FAKE_EXE ?? join(ROOT, 'target', 'debug', 'fake-cli.exe')
const EXPECTED_VERSION = process.env.SYMVOULIO_EXPECTED_VERSION ?? JSON.parse(readFileSync(join(ROOT, 'package.json'), 'utf8')).version
/** The page origin the app loads: the vite dev server (default), or the bundled frontend of a release-frontend build
 *  (custom-protocol), e.g. SYMVOULIO_E2E_BASE=http://tauri.localhost. */
export const BASE = process.env.SYMVOULIO_E2E_BASE ?? process.env.SYMVOULIO_DEV_BASE ?? 'http://localhost:5173'

export interface LaunchOptions {
  /** FAKE_CLI_SCENARIO for every provider call: ok (default) | auth_fail | malformed | slow | crash | … */
  scenario?: string
  /** Reuse a data dir (settings, workspace, WebView2 profile); default: a new temp dir. */
  dataDir?: string
  /** Extra environment for the app process. */
  env?: Record<string, string>
  /** Mark the first-run setup as done (skips the wizard). Default true. */
  setupDone?: boolean
  port?: number
}
export interface App {
  page: Page
  browser: Browser
  proc: ChildProcess
  dataDir: string
  /** The default workspace inside dataDir: runs live in <workspace>/runs/<id>. */
  workspace: string
  /** FAKE_CLI_LOG: one JSON line per provider invocation (argv). */
  fakeLog: string
  options: LaunchOptions
  /** Stop the app; the data dir is removed unless `keepData` (relaunch keeps it). */
  stop(keepData?: boolean): Promise<void>
}

let nextPort = 9400
async function cdpUp(port: number, ms: number) {
  const end = Date.now() + ms
  while (Date.now() < end) {
    try { if ((await fetch(`http://127.0.0.1:${port}/json/version`)).ok) return true } catch { /* not yet */ }
    await new Promise((r) => setTimeout(r, 300))
  }
  return false
}

/** L55: the scenarios the fake CLI actually ran, one entry per provider call ("" = default). */
export function scenariosRun(app: App): string[] {
  const f = `${app.fakeLog}.scenarios`
  if (!existsSync(f)) return []
  return readFileSync(f, 'utf8').split(/\r?\n/).filter(Boolean).map((l) => JSON.parse(l) as string)
}

/** Tauri injects IPC even into WebView2 error documents; require the mounted app on its own origin. */
async function waitForAppReady(page: Page) {
  const origin = new URL(BASE).origin
  const wait = () => page.waitForFunction((expectedOrigin) =>
    location.origin === expectedOrigin &&
    document.querySelector('#root [data-testid="main"]') !== null &&
    '__TAURI_INTERNALS__' in window,
  origin, { timeout: 90_000 })
  let failure: unknown
  try { await wait(); return } catch (e) { failure = e }
  // Give WebView2's own retry the full wait first. A refused dev-server request may need one explicit retry.
  if (!process.env.SYMVOULIO_E2E_BASE && /^(chrome-error:|about:blank)/.test(page.url())) {
    try { await page.goto(BASE, { waitUntil: 'commit', timeout: 90_000 }) } catch (e) { failure = e }
    try { await wait(); return } catch (e) { failure = e }
  }
  const url = page.url()
  const kind = url.startsWith('chrome-error:') ? 'WebView2 error page'
    : url === 'about:blank' ? 'about:blank' : 'unready app page'
  throw new Error(`App readiness timed out on ${kind}: ${url} (expected origin ${origin})`, { cause: failure })
}

export async function launchApp(options: LaunchOptions = {}): Promise<App> {
  if (!existsSync(APP_EXE)) throw new Error(`debug app missing: ${APP_EXE} (cargo build -p symvoulio)`)
  if (!existsSync(FAKE_EXE)) throw new Error(`fake CLI missing: ${FAKE_EXE} (cargo build -p symvoulio-core --bin fake-cli)`)
  const dataDir = options.dataDir ?? mkdtempSync(join(tmpdir(), 'symvoulio-e2e-'))
  mkdirSync(join(dataDir, 'webview2'), { recursive: true })
  const fakeLog = join(dataDir, 'fake-cli.log')
  if (!existsSync(fakeLog)) writeFileSync(fakeLog, '')
  const port = options.port ?? nextPort++
  const env: Record<string, string> = {
    ...(process.env as Record<string, string>),
    SYMVOULIO_DATA_DIR: dataDir,
    WEBVIEW2_USER_DATA_FOLDER: join(dataDir, 'webview2'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    SYMVOULIO_FAKE_CLI: FAKE_EXE,
    FAKE_CLI_LOG: fakeLog,
    ...(options.scenario ? { FAKE_CLI_SCENARIO: options.scenario } : {}),
    ...(options.env ?? {}),
  }
  // L55: a scenario passed through env would be dropped by the leak guard below; fail loudly instead of running the default.
  if (options.env && 'FAKE_CLI_SCENARIO' in options.env && !options.scenario) {
    throw new Error('launchApp: pass the fake scenario as options.scenario, not through options.env.FAKE_CLI_SCENARIO')
  }
  if (!options.scenario) delete env.FAKE_CLI_SCENARIO // leak guard: never inherit a scenario from the parent env
  const proc = spawn(APP_EXE, [], { env, stdio: 'ignore' })
  console.log(`desktop PID=${proc.pid} data=${dataDir}`)
  if (!(await cdpUp(port, 30_000))) { killTree(proc); throw new Error('the app did not expose CDP') }
  const browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`)
  const page = browser.contexts()[0].pages()[0]
  try {
    await waitForAppReady(page)
    const info = await ipc<{ version: string }>(page, 'app_info')
    if (info.version !== EXPECTED_VERSION) throw new Error(`Desktop binary version ${info.version}; expected ${EXPECTED_VERSION}: ${APP_EXE}`)
    console.log(`desktop version=${info.version}`)
    // Store trace seam: the app's store appends every applied/ignored snapshot and event (with generation, revision and
    // alias/id mapping) to window.__symvoulioTrace when that array exists. The harness creates it on every page load.
    await page.addInitScript(() => { (window as any).__symvoulioTrace = [] })
    await page.evaluate(() => { (window as any).__symvoulioTrace ??= [] })
    if (options.setupDone !== false) {
      await page.evaluate(() => localStorage.setItem('symvoulio.setup', JSON.stringify({ setupDone: true })))
      await page.goto(`${BASE}/reviews`, { waitUntil: 'commit', timeout: 90_000 })
      await waitForAppReady(page)
    }
    // CDP can type into a background WebView. Native SendKeys cannot: AppActivate
    // may report success while another process stays in front. Establish and
    // verify foreground ownership before any test drives native keyboard input.
    console.log(execFileSync('powershell.exe', ['-NoProfile', '-STA', '-File', join(ROOT, 'tests', 'desktop', 'activate-app.ps1'), '-TargetId', String(proc.pid)], { encoding: 'utf8', windowsHide: true }).trim())
  } catch (e) {
    await browser.close().catch(() => {})
    killTree(proc)
    throw e
  }
  // R07b classification aid: record every main-frame navigation and page error; written to the test's output dir
  // (navigations.txt) when the test did not pass, so an unexplained «execution context was destroyed» leaves a trace.
  const t0 = Date.now()
  const navLog: string[] = []
  page.on('framenavigated', (f) => { if (f === page.mainFrame()) navLog.push(`${Date.now() - t0}ms navigated ${f.url()}`) })
  page.on('pageerror', (e) => navLog.push(`${Date.now() - t0}ms pageerror ${e.message}`))
  page.on('console', (m) => { if (m.type() === 'error') navLog.push(`${Date.now() - t0}ms console.error ${m.text()}`) })
  const app: App = {
    page, browser, proc, dataDir, workspace: join(dataDir, 'workspace'), fakeLog, options,
    async stop(keepData = false) {
      try {
        // Written on every stop inside a test: in a test's finally block the failure status is not set yet, so a
        // status check here would never fire. The files are small (one line per navigation and store entry).
        const info = test.info()
        writeFileSync(info.outputPath(`navigations-${port}.txt`), navLog.join('\n') + '\n')
        const trace = await page.evaluate(() => (window as any).__symvoulioTrace ?? null).catch((e) => `unreadable: ${e}`)
        writeFileSync(info.outputPath(`store-trace-${port}.json`), JSON.stringify(trace, null, 1))
      } catch { /* not inside a test */ }
      await browser.close().catch(() => {}); killTree(proc)
      if (!keepData && !process.env.SYMVOULIO_E2E_KEEP) {
        await new Promise((r) => setTimeout(r, 800))
        try { rmSync(dataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 300 }) } catch { /* WebView2 may still hold a file */ }
      }
    },
  }
  return app
}

/** Stop the app and start it again on the same data dir (settings, runs, WebView2 profile), optionally changing the
 *  fake CLI scenario. Models an app restart. */
export async function relaunch(app: App, changes: Partial<LaunchOptions> = {}): Promise<App> {
  await app.stop(true)
  await new Promise((r) => setTimeout(r, 1000))
  return launchApp({ ...app.options, ...changes, dataDir: app.dataDir, port: undefined, setupDone: false })
}

/** Call a backend command over the real IPC, from inside the app's page. */
export async function ipc<T = unknown>(page: Page, cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  return page.evaluate(([c, a]) => (window as any).__TAURI_INTERNALS__.invoke(c, a), [cmd, args] as const) as Promise<T>
}

function killTree(proc: ChildProcess) {
  if (proc.pid && proc.exitCode === null) {
    try { execFileSync('taskkill', ['/T', '/F', '/PID', String(proc.pid)], { stdio: 'ignore' }) } catch { /* already gone */ }
  }
}
