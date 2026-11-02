// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian

// Contract checks from the user's side; no product edits. Run:  npx jiti tests/ct_v02_field.check.ts  (exit 1 on any FAIL)


//             also after a settings save (the sample round-trip saves settings) and after a relaunch.


import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { createJiti } from 'jiti'

const results: { name: string; ok: boolean }[] = []
let unhandled: string[] = []
process.on('unhandledRejection', reason => { unhandled.push(String(reason)) })
async function test(name: string, fn: () => void | Promise<void>) {
  unhandled = []
  try { await fn(); results.push({ name, ok: true }); console.log(`PASS ${name}`) }
  catch (e) { results.push({ name, ok: false }); console.log(`FAIL ${name}\n     ${(e instanceof Error ? e.message : String(e)).split('\n').slice(0, 6).join(' | ')}`) }
}
const tick = (ms = 0) => new Promise(r => setTimeout(r, ms))
const work = mkdtempSync(join(tmpdir(), 'ct-field-'))
const en = JSON.parse(readFileSync('src/i18n/en.json', 'utf8'))
const lookup = (key: string) => key.split('.').reduce((o: any, k) => o?.[k], en) as string | undefined
const t = (key: string, opts: Record<string, unknown> = {}) => (lookup(key) ?? key).replace(/\{\{(\w+)\}\}/g, (_, k) => String(opts[k] ?? ''))

function load(file: string, dependencies: Record<string, unknown>) {
  const compiler = createJiti(import.meta.url, { fsCache: false })
  const js = compiler.transform({ source: readFileSync(resolve(file), 'utf8'), filename: resolve(file), ts: true, jsx: { runtime: 'automatic' } })
  const exports: Record<string, any> = {}
  const req = (name: string) => { assert.ok(name in dependencies, `unexpected dependency: ${name}`); return dependencies[name] }
  new Function('require', 'exports', 'module', 'jitiImport', js)(req, exports, { exports }, async (name: string) => req(name))
  return exports
}


type El = { type: unknown; props: Record<string, any> }
function find(node: any, pred: (e: El) => boolean, out: El[] = []): El[] {
  if (node == null || typeof node !== 'object') return out
  if (Array.isArray(node)) { node.forEach(n => find(n, pred, out)); return out }
  if (node.props && pred(node)) out.push(node)
  if (node.props?.children !== undefined) find(node.props.children, pred, out)
  return out
}
const textOf = (node: any): string => node == null || typeof node === 'boolean' ? '' : typeof node === 'object' ? (Array.isArray(node) ? node.map(textOf).join('') : textOf(node.props?.children)) : String(node)
/** Render the production ExportPackage with a minimal hooks runtime so a click can run the real onClick handlers. */
function mountExport(backend: Record<string, unknown>, run: Record<string, unknown>, snapshot: unknown) {
  const slots: unknown[] = []
  let cursor = 0
  let renderRequested = false
  const useState = (initial: unknown) => {
    const i = cursor++
    if (!(i in slots)) slots[i] = initial
    return [slots[i], (v: unknown) => { slots[i] = typeof v === 'function' ? (v as any)(slots[i]) : v; renderRequested = true }]
  }
  const Passthrough = (props: any) => props.children
  const jsx = (type: any, props: any) => ({ type, props })
  const deps: Record<string, unknown> = {
    '../state/store': { matchesRun: (r: any, id: any) => r.id === id, useStore: (sel: any) => sel({ runs: [run], workspace: join(work, 'workspace') }) },
    react: { useState }, 'react/jsx-runtime': { jsx, jsxs: jsx, Fragment: Passthrough },
    'react-router': { useNavigate: () => () => {}, useParams: () => ({ id: run.id }) }, 'react-i18next': { useTranslation: () => ({ t }) },
    'motion/react': { motion: { div: 'div' } },
    'lucide-react': new Proxy({}, { get: () => 'icon' }), '@symvoulio/ui/components': { Button: 'Button', Card: 'Card' },
    '../components/Page': { Page: 'Page' }, '../lib/runState': { wasStopped: () => true },
    '../services/backend': { backend }, '../services/env': { isDesktop: true }, '../services/useRunData': { useRunData: () => ({ snapshot }) },
  }
  const { ExportPackage } = load('src/screens/S10Export.tsx', deps)
  const render = () => { cursor = 0; renderRequested = false; return ExportPackage() }
  let tree = render()
  return {
    tree: () => tree,
    async click(testId: string) {
      const [button] = find(tree, e => e.props['data-testid'] === testId)
      assert.ok(button, `no element ${testId}`)
      if (!button.props.disabled) { await button.props.onClick(); await tick(5) }
      tree = render(); if (renderRequested) tree = render()
    },
    result: () => { const [r] = find(tree, e => e.props['data-testid'] === 'export-result'); return r ? textOf(r) : '' },
    runPath: () => { const [r] = find(tree, e => e.props['data-testid'] === 'run-path'); return r ? textOf(r) : '' },
  }
}
const sampleRun = { id: 'sample', sample: true, name: 'Northwind hub expansion — business case', done: 3, state: 'completed', document: { name: 'business_case_northwind.md' } }
/** Backend double whose export really writes files, and whose Open folder only succeeds on an existing folder. */
function exportBackend() {
  const calls: { op: string; args: unknown[] }[] = []
  return {
    calls,
    backend: {
      exportRun: async (runId: string, partial: boolean) => {
        calls.push({ op: 'exportRun', args: [runId, partial] })
        const out = join(work, 'workspace', 'runs', runId, 'output'); mkdirSync(join(out, 'final'), { recursive: true })
        writeFileSync(join(out, 'final', 'final_document.md'), '# exported\n')
        return { path: out, tree: ['final/final_document.md'] }
      },
      openFolder: async (path: string) => { calls.push({ op: 'openFolder', args: [path] }); if (!existsSync(path)) throw new Error(`folder does not exist: ${path}`) },
    },
  }
}
const CLAIM = /exported|opens? in explorer|folder opens|package (is )?(written|exported)/i
const DISCLAIM = /unsupported|not supported|not available|no files|cannot|can't|prototype|sample.*(only|read)/i
await test('sample Export: a success claim is backed by files that exist on disk', async () => {
  const m = exportBackend()
  const ui = mountExport(m.backend, sampleRun, undefined)
  await ui.click('do-export')
  const said = ui.result()
  const claimed = CLAIM.test(said) && !DISCLAIM.test(said)
  if (!claimed) return // honest «unsupported» path: nothing was claimed
  const exp = m.calls.find(c => c.op === 'exportRun')
  assert.ok(exp, `UI says «${said}» but the export backend operation was never called`)
  const dir = join(work, 'workspace', 'runs', String(exp.args[0]), 'output')
  assert.ok(existsSync(dir) && readdirSync(dir, { recursive: true }).length > 0, `UI says «${said}» but no files exist at ${dir}`)
})
await test('sample Open folder: a success claim is backed by a real folder open of an existing path', async () => {
  const m = exportBackend()
  const ui = mountExport(m.backend, sampleRun, undefined)
  await ui.click('do-export')            // a real export, if the product offers one, creates the folder
  await ui.click('open-folder')
  const said = ui.result()
  const claimed = /opens?|opened/i.test(said) && !DISCLAIM.test(said)
  if (!claimed) return
  const open = m.calls.find(c => c.op === 'openFolder')
  assert.ok(open, `UI says «${said}» but openFolder was never invoked`)
  assert.ok(existsSync(String(open.args[0])), `openFolder was called on a path that does not exist: ${open.args[0]}`)
})
await test('sample Open folder before any export never claims the folder opened', async () => {
  const m = exportBackend()
  const ui = mountExport(m.backend, sampleRun, undefined)
  await ui.click('open-folder')
  const said = ui.result()
  const claimed = /opens?|opened/i.test(said) && !DISCLAIM.test(said)
  if (claimed) { const open = m.calls.find(c => c.op === 'openFolder'); assert.ok(open && existsSync(String(open.args[0])), `UI says «${said}» but nothing was opened (calls: ${JSON.stringify(m.calls)})`) }
})
await test('control: a real (non-sample) desktop run really exports and opens', async () => {
  const m = exportBackend()
  const run = { ...sampleRun, id: 'run-real', sample: false, name: 'real' }
  const ui = mountExport(m.backend, run, { id: 'real-backend-id', outputTree: ['final/x.md'] })
  await ui.click('do-export'); await ui.click('open-folder')
  assert.deepEqual(m.calls.map(c => c.op), ['exportRun', 'openFolder'])
  assert.ok(existsSync(String(m.calls[1].args[0])))
})


const pty = readFileSync('src-tauri/src/pty.rs', 'utf8')
function buildShellHarness() {
  const from = pty.indexOf('fn choose_shell'); const to = pty.indexOf('fn home_directory')
  assert.ok(from > 0 && to > from, 'choose_shell not found')
  writeFileSync(join(work, 'shell.rs'), `#![allow(dead_code)]
struct ShellInfo { os: String, pwsh: bool, powershell: bool, chosen: String, available: Vec<String> }
${pty.slice(from, to)}
fn main() {
  let a: Vec<String> = std::env::args().collect();
  let saved = if a[1] == "-" { None } else { Some(a[1].as_str()) };
  let (pwsh, powershell) = (a[2] == "1", a[3] == "1");
  let info = ShellInfo { os: "windows".into(), pwsh, powershell, chosen: if powershell { "powershell".into() } else { "pwsh".into() }, available: vec![] };
  match choose_shell(saved, &info) { Ok(s) => println!("OK:{s}"), Err(e) => println!("ERR:{e}") }
}
`)
  const r = spawnSync('rustc', ['--edition', '2021', '-o', join(work, 'shell.exe'), join(work, 'shell.rs')], { encoding: 'utf8' })
  assert.equal(r.status, 0, `choose_shell failed to compile in isolation: ${r.stderr.slice(0, 500)}`)
}
let shellError: string | undefined
try { buildShellHarness() } catch (e) { shellError = e instanceof Error ? e.message : String(e) }
/** What the app's choose_shell makes of a saved preference. null = nothing saved. */
function chooseShell(saved: string | null | undefined, pwsh = false, powershell = true) {
  assert.equal(shellError, undefined, String(shellError))
  const r = spawnSync(join(work, 'shell.exe'), [saved == null ? '-' : saved, pwsh ? '1' : '0', powershell ? '1' : '0'], { encoding: 'utf8' })
  return r.stdout.trim()
}
await test('backend: no saved preference and an empty saved preference both give a working shell when PowerShell is present', () => {
  assert.match(chooseShell(null), /^OK:/)
  assert.match(chooseShell(''), /^OK:/, `empty preference gave ${chooseShell('')}`)
  assert.match(chooseShell('', true, false), /^OK:/, 'empty preference with only pwsh 7')
  assert.match(chooseShell('powershell'), /^OK:/)
})

const { toPolicy } = await import('../src/services/policy')
const { DEFAULT_POLICY } = await import('../src/lib/review')
function settingsPayload(shell: { claude: string | null; codex: string | null }) {
  return { workspace: join(work, 'workspace'), author: 'claude', policy: toPolicy(DEFAULT_POLICY), shell }
}
/** One app session: the real store + adapter in desktop mode over a fake IPC invoke. localStorage persists across sessions. */
const disk = new Map<string, string>()
const g = globalThis as any
async function session<T>(handler: (cmd: string, args: any) => unknown, body: (m: { store: any; adapter: any; files: any; backendMod: any }) => Promise<T>) {
  const saved = { window: Object.getOwnPropertyDescriptor(g, 'window'), location: Object.getOwnPropertyDescriptor(g, 'location') }
  const localStorage = { getItem: (k: string) => disk.get(k) ?? null, setItem: (k: string, v: string) => { disk.set(k, v) }, removeItem: (k: string) => { disk.delete(k) } }
  Object.defineProperty(g, 'location', { value: { search: '' }, configurable: true })
  Object.defineProperty(g, 'window', { value: { __TAURI_INTERNALS__: { invoke: async (c: string, a: any) => handler(c, a), transformCallback: () => 1 }, setTimeout, localStorage, addEventListener() {} }, configurable: true })
  g.localStorage = localStorage
  const fresh = createJiti(import.meta.url, { fsCache: false, moduleCache: false })
  try {
    const store = await fresh.import<any>('../src/state/store')
    for (let i = 0; i < 16; i++) await tick(25) // the adapter connects and loads settings and runs on later ticks
    return await body({ store, adapter: await fresh.import<any>('../src/services/storeAdapter'), files: await fresh.import<any>('../src/services/files'), backendMod: await fresh.import<any>('../src/services/backend') })
  } finally {
    for (const [k, d] of Object.entries(saved)) if (d) Object.defineProperty(g, k, d); else delete g[k]
    delete g.localStorage
  }
}
await test('settings round-trip never turns automatic shell selection into an unusable explicit one', async () => {
  const sent: any[] = []
  const handler = (cmd: string, args: any) => {
    if (cmd === 'settings_get') return settingsPayload({ claude: null, codex: null })
    if (cmd === 'settings_set') { sent.push(args.settings ?? args); return args.settings ?? args }
    if (cmd === 'detect_providers') return []
    if (cmd === 'runs_list') return []
    return undefined
  }
  await session(handler, async ({ store }) => {
    // finishSetup / policy changes / author changes all save settings; the sample round-trip does too.
    store.setPolicy?.(store.getState().policy)
    store.finishSetup?.()
    await tick(50)
    assert.ok(sent.length > 0, `no settings save observed (exports: ${Object.keys(store).filter(k => /setup|polic|author|setting/i.test(k)).join(',')})`)
  })
  for (const settings of sent) for (const panel of ['claude', 'codex']) {
    const value = settings.shell?.[panel]
    assert.match(chooseShell(value), /^OK:/, `saved ${panel} shell ${JSON.stringify(value)} makes the panel «${chooseShell(value)}»`)
  }
})
await test('panel opens a working shell when the saved preference is "" (no «Shell is unavailable» on screen)', async () => {
  const displayed: string[] = []
  const opened: any[] = []
  const handler = (cmd: string, args: any) => {
    if (cmd === 'settings_get') return settingsPayload({ claude: '', codex: '' })
    if (cmd === 'pty_open') {
      opened.push(args.shell)
      const r = chooseShell(args.shell)
      if (r.startsWith('ERR:')) throw r.slice(4)
      return { shell: r.slice(3), reattached: false }
    }
    return undefined
  }
  const host = new EventTarget(); const effects: (() => unknown)[] = []
  class XTerm { options: any = {}; cols = 80; rows = 24; loadAddon() {} open() {} dispose() {} onData() {} write(s: string) { displayed.push(s) } textarea = undefined }
  class Observer { observe() {} disconnect() {} }
  const saved = { document: g.document, MutationObserver: g.MutationObserver, ResizeObserver: g.ResizeObserver }
  Object.assign(g, { document: { documentElement: { classList: { contains: () => false } } }, MutationObserver: Observer, ResizeObserver: Observer })
  g.window = { __TAURI_INTERNALS__: { invoke: async (c: string, a: any) => handler(c, a), transformCallback: () => 1 } }
  try {
    const { PALETTE, xtermTheme } = await import('../src/lib/terminalTheme')
    const { pasteDecision } = await import('../src/lib/terminalInput')
    const { desktopBackend } = load('src/services/backend.ts', {
      './canned': { cannedBackend: {} }, './env': { isDesktop: true },
      '@tauri-apps/api/core': { invoke: async (c: string, a: any) => handler(c, a), Channel: class { onmessage: unknown } },
    })
    const { Terminal } = load('src/frame/Terminal.tsx', {
      react: { useEffect: (fn: () => unknown) => effects.push(fn), useRef: (v: unknown) => ({ current: v }), useState: () => [false] },
      'react/jsx-runtime': { jsx: (_: unknown, props: { ref: { current: unknown } }) => { props.ref.current = host } },
      '@xterm/xterm': { Terminal: XTerm }, '@xterm/addon-fit': { FitAddon: class { fit() {} } },
      '../services/panelShells': { setPanelShell() {} }, '../services/backend': { backend: desktopBackend },
      '../lib/terminalTheme': { PALETTE, xtermTheme }, '../lib/terminalInput': { pasteDecision },
      '../state/install': { installExited() {}, installPainted() {}, installTimeout() {} }, '../i18n': { i18n: { t: () => 'refused' } },
    })
    Terminal({ lines: [], mode: 'raw', prompt: null, pending: null, label: 'x', testId: 'term-claude', contextKey: 'x', live: 'claude' })
    const cleanups = effects.map(fn => fn())
    await tick(150)
    for (const c of cleanups) if (typeof c === 'function') c()
  } finally { Object.assign(g, saved); delete g.window }
  assert.ok(opened.length > 0, 'the panel never asked for a shell')
  assert.ok(!displayed.some(s => /unavailable/i.test(s)), `panel shows: ${JSON.stringify(displayed.filter(s => /unavailable/i.test(s)))}`)
})
await test('relaunch: persisted "" settings from a previous session still give working panels', async () => {
  // The previous session left {"claude":"","codex":""} on disk (what the field machine had); the next session reads it.
  const handler = (cmd: string, args: any) => cmd === 'settings_get' ? settingsPayload({ claude: '', codex: '' }) : cmd === 'detect_providers' || cmd === 'runs_list' ? [] : cmd === 'settings_set' ? (args.settings ?? args) : undefined
  await session(handler, async ({ store }) => {
    const names = store.getState().shellName
    for (const panel of ['claude', 'codex']) assert.match(chooseShell(names[panel] === '' ? '' : names[panel]), /^OK:/, `${panel}: ${JSON.stringify(names[panel])}`)
  })
})


await test('a review that failed at start is still listed after a restart', async () => {
  const journal: { cmd: string; args: any }[] = []
  const created: any[] = []
  const KNOWN = new Set(['plugin:event|listen', 'raw_output_set', 'shell_info', 'run_export', 'open_folder', 'run_pause', 'run_resume', 'run_stop', 'run_restart', 'run_decide', 'run_decide_issue', 'settings_get', 'settings_set', 'detect_providers', 'runs_list', 'stage_file', 'stage_discard', 'run_start', 'run_get', 'run_subscribe', 'pty_open', 'pty_write', 'pty_resize', 'pty_paste', 'app_info', 'run_delete'])
  const NAME = 'CT failed-at-start review'
  const handler = (cmd: string, args: any) => {
    if (cmd === 'settings_get') return settingsPayload({ claude: null, codex: null })
    if (cmd === 'detect_providers') return []
    if (cmd === 'stage_file') return { token: 't', name: args.name, size: 1, kind: 'md', chars: 1, error: 'Text is too large to review (limit exceeded)' }
    if (cmd === 'stage_discard') return undefined
    if (cmd === 'run_start') { const id = `backend-${created.length + 1}`; created.push({ id, name: args.input.name, createdAt: new Date().toISOString(), state: 'running', roundsDone: 0 }); throw new Error('unexpected successful start') }
    if (cmd === 'runs_list') {
      // What a restarted backend lists: runs it created, plus anything the app recorded through any other command that names the review.
      const recorded = journal.filter(j => !KNOWN.has(j.cmd) && JSON.stringify(j.args).includes(NAME)).map((j, i) => ({ id: `recorded-${i}`, name: NAME, createdAt: new Date().toISOString(), state: 'failed', roundsDone: 0 }))
      return [...created, ...recorded]
    }
    journal.push({ cmd, args })
    return undefined
  }
  await session(handler, async ({ store, files }) => {
    const document = files.registerFile({ name: 'huge.md', size: 1, type: 'md' }, new Blob(['x']))
    const id = store.startRun({ name: NAME, author: 'claude', document, supporting: [], objective: 'o', criteria: [], policy: store.getState().policy })
    await tick(100)
    const row = store.getState().runs.find((r: any) => r.id === id)
    assert.equal(row?.state, 'failed', 'precondition: the start failure shows as a failed row in this session')
  })
  // Restart: new store, new adapter, same disk (localStorage) and same backend.
  await session(handler, async ({ store }) => {
    await tick(100)
    const names = store.getState().runs.map((r: any) => `${r.name}:${r.state}`)
    const listed = store.getState().runs.find((r: any) => r.name === NAME)
    assert.ok(listed, `after restart the failed review is gone; reviews listed: ${JSON.stringify(names)}; commands after the failure: ${JSON.stringify(journal.map(j => j.cmd))}`)
    assert.equal(listed.state, 'failed')
  })
})
await test('control: a review the backend really started is listed after a restart (harness can see a listing)', async () => {
  const snapshot = { generation: 1, revision: 1, id: 'backend-ok', name: 'CT started review', createdAt: new Date().toISOString(), author: 'claude', reviewer: 'codex', policy: toPolicy(DEFAULT_POLICY), objective: '', criteria: [], state: 'running', stopReason: null, lastError: null, roundsDone: 0, documents: [], rounds: [], issues: [], humanQuestions: [], counts: { critical: 0, high: 0, medium: 0, low: 0 }, decision: null, decisionLog: [], outputTree: [], primaryName: 'x.md', supportingNames: [], phase: null, pendingRevision: null }
  const handler = (cmd: string) => {
    if (cmd === 'settings_get') return settingsPayload({ claude: null, codex: null })
    if (cmd === 'stage_file') return { token: 't', name: 'x.md', size: 1, kind: 'md', chars: 1, error: null }
    if (cmd === 'run_start' || cmd === 'run_get') return snapshot
    if (cmd === 'runs_list') return [{ id: snapshot.id, name: snapshot.name, createdAt: snapshot.createdAt, state: 'running', roundsDone: 0 }]
    if (cmd === 'detect_providers') return []
    return undefined
  }
  await session(handler, async ({ store, files }) => {
    store.startRun({ name: snapshot.name, author: 'claude', document: files.registerFile({ name: 'x.md', size: 1, type: 'md' }, new Blob(['x'])), supporting: [], objective: 'o', criteria: [], policy: store.getState().policy })
    await tick(100)
  })
  await session(handler, async ({ store }) => {
    await tick(100)
    assert.ok(store.getState().runs.some((r: any) => r.name === snapshot.name), JSON.stringify(store.getState().runs.map((r: any) => r.name)))
  })
})
await test('control: a failure before start does not fabricate a listing when nothing was started', async () => {
  await session(() => undefined, async ({ store }) => { assert.deepEqual(store.getState().runs, []) })
})

try { rmSync(work, { recursive: true, force: true }) } catch { /* best effort */ }
const failed = results.filter(r => !r.ok)
console.log(`\n${results.length - failed.length}/${results.length} passed; ${failed.length} failed`)
for (const f of failed) console.log(`  FAILED: ${f.name}`)
process.exit(failed.length ? 1 : 0)
