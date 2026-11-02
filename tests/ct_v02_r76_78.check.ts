// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian

//   R76  PowerShell completion status is truthful: no guessed number is ever shown as an exit code.
//   R77  Clipboard paste into the terminal is delivered (also into a waiting/busy prompt); app command buttons still
//        refuse while input is pending or busy; every refusal is visible; repeated S2 Paste yields one login command.
//   R78  A login/install path with U+2019 (and U+2018, U+201A, U+201B) executes through real PowerShell.
// Run:  npx jiti tests/ct_v02_r76_78.check.ts      (exit 1 if any case fails; every case prints PASS/FAIL)
// R76/R78 execute the checked-out production source (the hook text and the real resolve_login_command compiled with
// rustc against a stubbed resolver) in real Windows PowerShell and pwsh 7 when present.
import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { createJiti } from 'jiti'

const compiler = createJiti(import.meta.url, { fsCache: false })
const pty = readFileSync('src-tauri/src/pty.rs', 'utf8')
const results: { name: string; ok: boolean; why?: string }[] = []
// A rejected backend promise that nothing handles is a silently discarded refusal: it fails the case that caused it.
let unhandled: string[] = []
process.on('unhandledRejection', reason => { unhandled.push(String(reason)) })
async function test(name: string, fn: () => void | Promise<void>) {
  unhandled = []
  try { await fn(); await new Promise(r => setTimeout(r, 20)); if (unhandled.length) throw new Error(`unhandled rejection (refusal silently discarded): ${unhandled[0]}`); results.push({ name, ok: true }); console.log(`PASS ${name}`) }
  catch (e) { const why = e instanceof Error ? e.message.split('\n').slice(0, 6).join(' | ') : String(e); results.push({ name, ok: false, why }); console.log(`FAIL ${name}\n     ${why}`) }
}
const shells = ['powershell', 'pwsh'].filter(s => spawnSync(s, ['-NoProfile', '-Command', '1'], { encoding: 'utf8' }).status === 0)
assert.ok(shells.includes('powershell'), 'real Windows PowerShell must be present on this station')
console.log('shells under test:', shells.join(', '))
function psRun(shell: string, script: string, env: Record<string, string> = {}) {
  return spawnSync(shell, ['-NoProfile', '-NonInteractive', '-EncodedCommand', Buffer.from(script, 'utf16le').toString('base64')], { encoding: 'utf8', env: { ...process.env, ...env } })
}

// ---------------------------------------------------------------- R76
function load(file: string, dependencies: Record<string, unknown>) {
  const source = readFileSync(resolve(file), 'utf8')
  const js = compiler.transform({ source, filename: resolve(file), ts: true, jsx: { runtime: 'automatic' } })
  const exports: Record<string, any> = {}
  const req = (name: string) => { assert.ok(name in dependencies, `unexpected dependency: ${name}`); return dependencies[name] }
  new Function('require', 'exports', 'module', 'jitiImport', js)(req, exports, { exports }, async (name: string) => req(name))
  return exports
}
const hook = pty.match(/r#"(\$global:SymvoulioOriginalPrompt[\s\S]*?)"#/)?.[1]
assert.ok(hook, 'production PowerShell prompt hook not found in pty.rs')
/** Each marker the hook wrote for one prompt: the text after «133;D;TOKEN;». */
function statusMarkers(shell: string, commands: string, fresh = true) {
  const script = `${hook}; $env:SYMVOULIO_PROMPT_TOKEN='TOK'; ${commands}`
  const r = psRun(shell, script)
  assert.equal(r.status, 0, `${shell} failed: ${r.stderr}`)
  void fresh
  return [...r.stdout.matchAll(/\x1b\]133;D;TOK;([^\x07]*)\x07/g)].map(m => m[1])
}
/** A marker is success iff it is exactly 0; otherwise failure, and any number shown must be a real native exit code. */
const P = 'prompt | Out-Null'
for (const shell of shells) {
  await test(`R76 ${shell}: native exit 7 is a failure carrying 7 (or no number)`, () => {
    const [m] = statusMarkers(shell, `cmd /c exit 7; ${P}`)
    assert.ok(m === '7', `expected 7, got ${JSON.stringify(m)}`)
  })
  await test(`R76 ${shell}: repeated native exit 7 never shows a guessed number (7 or none, not 1, not 0)`, () => {
    const m = statusMarkers(shell, `cmd /c exit 7; ${P}; cmd /c exit 7; ${P}; cmd /c exit 7; ${P}`)
    assert.equal(m.length, 3)
    for (const x of m) assert.ok(x === '7' || x === '', `failure shown as ${JSON.stringify(x)} in ${JSON.stringify(m)}`)
  })
  await test(`R76 ${shell}: native exit 7 then native exit 0 followed by Write-Error is a failure with no number`, () => {
    const m = statusMarkers(shell, `cmd /c exit 7; ${P}; cmd /c exit 0; Write-Error boom 2>$null; ${P}`)
    assert.equal(m[0], '7')
    assert.notEqual(m[1], '0', 'a failed command was reported as success (0)')
    assert.equal(m[1], '', `no native code exists; got ${JSON.stringify(m[1])}`)
  })
  await test(`R76 ${shell}: Write-Error in a fresh session is a failure with no number`, () => {
    const [m] = statusMarkers(shell, `cmd /c exit 0; Write-Error boom 2>$null; ${P}`)
    assert.equal(m, '')
  })
  await test(`R76 ${shell}: failing cmdlet after exit 7 shows no number (not the stale 7, not 1)`, () => {
    const m = statusMarkers(shell, `cmd /c exit 7; ${P}; Get-Item C:\\ct-no-such-path-xyz 2>$null; ${P}`)
    assert.equal(m[0], '7'); assert.equal(m[1], '')
  })
  await test(`R76 ${shell}: successful cmdlet after exit 7 is success (0)`, () => {
    const m = statusMarkers(shell, `cmd /c exit 7; ${P}; Write-Output ok | Out-Null; ${P}`)
    assert.deepEqual(m, ['7', '0'])
  })
  await test(`R76 ${shell}: successful cmdlet in a fresh session is success (0)`, () => {
    assert.deepEqual(statusMarkers(shell, `Write-Output ok | Out-Null; ${P}`), ['0'])
  })
  await test(`R76 ${shell}: exit 0 native after exit 7 is success (0)`, () => {
    assert.deepEqual(statusMarkers(shell, `cmd /c exit 7; ${P}; cmd /c exit 0; ${P}`), ['7', '0'])
  })
  await test(`R76 ${shell}: the hook preserves $LASTEXITCODE for the user`, () => {
    const r = psRun(shell, `${hook}; $env:SYMVOULIO_PROMPT_TOKEN='TOK'; cmd /c exit 7; ${P}; Write-Output ("LEC=" + $LASTEXITCODE)`)
    assert.match(r.stdout, /LEC=7/)
  })
}
await test('R76 UI: wording never calls a missing/guessed status an «exit code», and 7 is shown as status', async () => {
  const en = JSON.parse(readFileSync('src/i18n/en.json', 'utf8'))
  const lookup = (key: string) => key.split('.').reduce((o: any, k) => o?.[k], en) as string
  const t = (key: string, opts: Record<string, unknown> = {}) => (lookup(key) ?? key).replace(/\{\{(\w+)\}\}/g, (_, k) => String(opts[k] ?? ''))
  let snapshot: any
  const deps: Record<string, unknown> = {
    react: { useSyncExternalStore: (_s: unknown, snap: () => unknown) => (snapshot = snap()) },
    './store': { getState: () => ({ shells: { claude: {} }, shellName: {}, conn: { claude: { installed: false }, codex: { installed: false } } }), pasteCommand: async () => {}, checkAgain: async () => {} },
    '../lib/installCommands': { installPlan: () => ({ ok: true, command: 'x' }) }, '../services/backend': { installEnvironment: async () => ({ os: 'windows' }) },
    '../services/env': { isDesktop: true }, '../services/panelShells': { getPanelShell: () => 'powershell' }, '../i18n': { i18n: { t } },
  }
  const install = load('src/state/install.ts', deps)
  ;(globalThis as any).document = { querySelector: () => null }
  const messages: string[] = []
  for (const code of [7, null]) {
    await install.startInstall('claude')
    await install.installExited('claude', code, 'last line')
    install.useInstall()
    messages.push(snapshot.claude.message)
  }
  assert.match(messages[0], /7/, `status 7 must be shown: ${messages[0]}`)
  for (const m of messages) assert.doesNotMatch(m, /exit code/i, `UI calls a status an «exit code»: ${m}`)
  assert.doesNotMatch(messages[1], /\d/, `no number may be shown when none exists: ${messages[1]}`)
  assert.doesNotMatch(String(lookup('s1.installUnavailable')), /exit code/i)
})

// ---------------------------------------------------------------- R78
const work = mkdtempSync(join(tmpdir(), 'ct-r78-'))
function buildLoginHarness() {
  const start = pty.indexOf('fn resolve_login_command')
  assert.ok(start > 0, 'resolve_login_command not found')
  let from = pty.lastIndexOf('#[cfg(windows)]', start)
  const esc = pty.indexOf('fn escape_powershell_literal')
  if (esc >= 0 && esc < start) from = pty.lastIndexOf('#[cfg(windows)]', esc)
  const end = pty.indexOf('pub fn validate_paste', start)
  const production = pty.slice(from, end)
  const src = `#![allow(dead_code)]
mod symvoulio_core {
  #[derive(Clone, Copy)] pub enum Provider { Codex, Claude }
  pub mod cli {
    use std::path::PathBuf;
    pub struct Cli { pub program: PathBuf, pub shim: Option<PathBuf>, pub prefix_args: Vec<String> }
    pub fn resolve(_p: super::Provider) -> Option<Cli> { Some(Cli { program: PathBuf::from(std::env::var_os("CT_PROGRAM").unwrap()), shim: None, prefix_args: vec![] }) }
  }
}
${production}
fn main() {
  let panel = std::env::var("CT_PANEL").unwrap_or("codex".into());
  let cmd = if panel == "codex" { "codex login" } else { "claude auth login" };
  print!("{}", resolve_login_command(&panel, cmd.to_string(), None));
}
`
  writeFileSync(join(work, 'harness.rs'), src)
  const r = spawnSync('rustc', ['--edition', '2021', '-o', join(work, 'harness.exe'), join(work, 'harness.rs')], { encoding: 'utf8' })
  assert.equal(r.status, 0, `production resolve_login_command failed to compile in isolation: ${r.stderr.slice(0, 600)}`)
  writeFileSync(join(work, 'target.rs'), 'fn main() { println!("RAN:{}", std::env::args().skip(1).collect::<Vec<_>>().join(" ")); }')
  const t = spawnSync('rustc', ['--edition', '2021', '-o', join(work, 'target-base.exe'), join(work, 'target.rs')], { encoding: 'utf8' })
  assert.equal(t.status, 0, t.stderr)
}
let harnessReady: string | undefined
try { buildLoginHarness() } catch (e) { harnessReady = e instanceof Error ? e.message : String(e) }
const dirs: [string, string][] = [
  ['U+2019', 'O\u2019Connor'], ['U+2018', 'O\u2018Connor'], ['U+201A', 'O\u201AConnor'], ['U+201B', 'O\u201BConnor'],
  ['ASCII apostrophe', "O'Connor"], ['all four curly and ASCII', "O\u2019C\u2018o\u201An\u201Bn'or"],
]
for (const shell of shells) for (const [label, name] of dirs) {
  for (const panel of ['codex', 'claude']) {
    await test(`R78 ${shell} ${panel}: executable under «${name}» (${label}) runs from the generated login command`, () => {
      assert.equal(harnessReady, undefined, String(harnessReady))
      const dir = join(work, `${shell}-${panel}-${label.replace(/\W/g, '')}`, name)
      mkdirSync(dir, { recursive: true })
      const exe = join(dir, panel === 'codex' ? 'codex.exe' : 'claude.exe')
      writeFileSync(exe, readFileSync(join(work, 'target-base.exe')))
      const gen = spawnSync(join(work, 'harness.exe'), [], { encoding: 'utf8', env: { ...process.env, CT_PROGRAM: exe, CT_PANEL: panel } })
      assert.equal(gen.status, 0, gen.stderr)
      const command = gen.stdout
      assert.ok(command.includes(name.replace(/'/g, "''")) || command.length > 0, 'a command was generated')
      const r = psRun(shell, command)
      assert.equal(r.status, 0, `generated command failed in ${shell}: ${command}\n${r.stderr.slice(0, 300)}`)
      assert.match(r.stdout, panel === 'codex' ? /RAN:login/ : /RAN:auth login/)
    })
  }
}

// ---------------------------------------------------------------- R77
const tick = () => new Promise(r => setTimeout(r, 0))
type Call = { command: string; args: any }
/** Backend double implementing the documented contract: guard refuses app commands while input is pending or busy. */
function fakeBackend(opts: { busy?: boolean } = {}) {
  const calls: Call[] = []
  const state = { pending: '', busy: !!opts.busy, delivered: [] as string[] }
  const invoke = async (command: string, args: any) => {
    calls.push({ command, args })
    if (command === 'pty_write') { state.pending += args.data; state.delivered.push(args.data); return }
    if (command === 'pty_paste') {
      const clipboard = args.clipboard === true
      if (!clipboard && (state.pending || state.busy)) throw 'The panel has pending input or is busy. Finish or clear it before installing.'
      state.pending += args.command; state.delivered.push(args.command); return
    }
  }
  return { calls, state, invoke }
}
async function withTerminal<T>(invoke: (c: string, a: any) => Promise<unknown>, body: (t: { onData(d: string): void; paste(text: string): Promise<boolean>; displayed: string[] }) => Promise<T>) {
  const { desktopBackend } = load('src/services/backend.ts', { './canned': { cannedBackend: {} }, './env': { isDesktop: true }, '@tauri-apps/api/core': { invoke } })
  const host = new EventTarget()
  const effects: (() => unknown)[] = []
  const displayed: string[] = []
  let onData: (d: string) => void = () => { throw new Error('onData missing') }
  class XTerm { options = {}; cols = 80; rows = 24; loadAddon() {} open() {} dispose() {} onData(cb: typeof onData) { onData = cb } write(s: string) { displayed.push(s) } }
  class Observer { observe() {} disconnect() {} }
  const g = globalThis as any
  const saved = { document: g.document, MutationObserver: g.MutationObserver, ResizeObserver: g.ResizeObserver }
  try {
    Object.assign(g, { document: { documentElement: { classList: { contains: () => false } } }, MutationObserver: Observer, ResizeObserver: Observer })
    const { PALETTE, xtermTheme } = await import('../src/lib/terminalTheme')
    const { pasteDecision } = await import('../src/lib/terminalInput')
    const { Terminal } = load('src/frame/Terminal.tsx', {
      react: { useEffect: (fn: () => unknown) => effects.push(fn), useRef: (v: unknown) => ({ current: v }), useState: () => [false] },
      'react/jsx-runtime': { jsx: (_: unknown, props: { ref: { current: unknown } }) => { props.ref.current = host } },
      '@xterm/xterm': { Terminal: XTerm }, '@xterm/addon-fit': { FitAddon: class { fit() {} } },
      '../services/panelShells': { setPanelShell() {} }, '../services/backend': { backend: desktopBackend },
      '../lib/terminalTheme': { PALETTE, xtermTheme }, '../lib/terminalInput': { pasteDecision },
      '../state/install': { installExited() {}, installPainted() {}, installTimeout() {} }, '../i18n': { i18n: { t: () => 'paste refused' } },
    })
    Terminal({ lines: [], mode: 'raw', prompt: null, pending: null, label: 'test', testId: 'test', contextKey: 'test', live: 'claude' })
    const cleanups = effects.slice(0, 2).map(fn => fn())
    const paste = async (text: string) => {
      const event = new Event('paste', { cancelable: true, bubbles: true })
      Object.defineProperty(event, 'clipboardData', { value: { getData: () => text } })
      host.dispatchEvent(event); await tick(); await tick()
      return event.defaultPrevented
    }
    try { return await body({ onData: d => onData(d), paste, displayed }) }
    finally { for (const c of cleanups.reverse()) if (typeof c === 'function') c() }
  } finally { Object.assign(g, saved) }
}
/** The app-command route (S2 Paste) through the real store and desktop adapter, with the same backend double. */
let currentInvoke: (c: string, a: any) => Promise<unknown> = async () => { throw new Error('no backend double installed') }
async function withStore<T>(invoke: (c: string, a: any) => Promise<unknown>, body: (s: any) => Promise<T>) {
  currentInvoke = invoke
  const g = globalThis as any
  const savedWindow = Object.getOwnPropertyDescriptor(g, 'window')
  const savedLocation = Object.getOwnPropertyDescriptor(g, 'location')
  Object.defineProperty(g, 'location', { value: { search: '' }, configurable: true })
  Object.defineProperty(g, 'window', { value: { __TAURI_INTERNALS__: { invoke: (c: string, a: any) => currentInvoke(c, a), transformCallback: () => 1 }, setTimeout, localStorage: { getItem: () => null, setItem() {} }, addEventListener() {} }, configurable: true })
  try {
    const mod = await compiler.import('../src/state/store')
    for (let i = 0; i < 20; i++) await new Promise(r => setTimeout(r, 25)) // the store connects its desktop adapter on a later tick
    return await body(mod)
  } finally {
    if (savedWindow) Object.defineProperty(g, 'window', savedWindow); else delete g.window
    if (savedLocation) Object.defineProperty(g, 'location', savedLocation); else delete g.location
  }
}

await test('R77 clipboard paste after typing «echo » appends exactly the pasted text', async () => {
  const b = fakeBackend()
  await withTerminal(b.invoke, async t => {
    for (const ch of 'echo ') t.onData(ch)
    await tick()
    assert.equal(b.state.pending, 'echo ')
    assert.equal(await t.paste('clipboard-argument'), true, 'paste event must be handled (cancelled)')
    assert.equal(b.state.pending, 'echo clipboard-argument', `delivered ${JSON.stringify(b.state.delivered)}; screen ${JSON.stringify(t.displayed.slice(-2))}`)
  })
})
await test('R77 clipboard paste into a waiting prompt (busy process) is delivered', async () => {
  const b = fakeBackend({ busy: true })
  await withTerminal(b.invoke, async t => {
    await t.paste('auth-code-1234')
    assert.equal(b.state.pending, 'auth-code-1234', `screen: ${JSON.stringify(t.displayed.slice(-2))}`)
    assert.ok(!t.displayed.some(s => /busy|pending/i.test(s)), 'a legitimate clipboard paste must not be refused')
  })
})
await test('R77 clipboard paste keeps exact bytes (spaces, quotes, unicode, 512 chars)', async () => {
  const b = fakeBackend()
  const text = `  a'b"c  \u2019 ${'x'.repeat(480)}  `
  await withTerminal(b.invoke, async t => { await t.paste(text); assert.equal(b.state.pending, text) })
})
await test('R77 clipboard paste is distinguishable on the wire from an app command (S2 Paste cannot bypass the guard)', async () => {
  const term = fakeBackend(); const app = fakeBackend()
  await withTerminal(term.invoke, async t => { await t.paste('echo x') })
  await withStore(app.invoke, async s => { await s.pasteCommand('claude', 'claude auth login', 'login'); await tick() })
  const a = term.calls.find(c => c.command === 'pty_paste'); const b = app.calls.find(c => c.command === 'pty_paste')
  assert.ok(a && b, 'both routes must reach pty_paste')
  const strip = (x: any) => { const { command: _c, ...rest } = x.args; return rest }
  const truthyExtras = Object.entries(strip(a)).filter(([k, v]) => v && !(k in strip(b) && strip(b)[k] === v))
  assert.ok(truthyExtras.length > 0, `clipboard ${JSON.stringify(strip(a))} is indistinguishable from app command ${JSON.stringify(strip(b))}`)
  assert.ok(!Object.entries(strip(b)).some(([k, v]) => v === true && k === 'clipboard'), 'the app command must never claim to be clipboard input')
})
await test('R77 app command (S2 Paste) is refused while input is pending, and the refusal is visible', async () => {
  const b = fakeBackend(); b.state.pending = 'half-typed'
  await withStore(b.invoke, async s => {
    await s.pasteCommand('claude', 'claude auth login', 'login'); await tick()
    assert.deepEqual(b.state.delivered, [], 'nothing may be delivered over pending input')
    assert.match(String(s.getState().shells.claude.pasteError ?? ''), /pending input|busy/i, 'refusal must be shown to the user')
  })
})
await test('R77 app command (S2 Paste) is refused while a process is busy, and the refusal is visible', async () => {
  const b = fakeBackend({ busy: true })
  await withStore(b.invoke, async s => {
    await s.pasteCommand('claude', 'claude auth login', 'login'); await tick()
    assert.deepEqual(b.state.delivered, [])
    assert.match(String(s.getState().shells.claude.pasteError ?? ''), /pending input|busy/i)
  })
})
await test('R77 repeated S2 Paste leaves exactly one login command, second attempt visibly refused', async () => {
  const b = fakeBackend()
  await withStore(b.invoke, async s => {
    await s.pasteCommand('claude', 'claude auth login', 'login'); await tick()
    await s.pasteCommand('claude', 'claude auth login', 'login'); await tick()
    await s.pasteCommand('claude', 'claude auth login', 'login'); await tick()
    assert.deepEqual(b.state.delivered, ['claude auth login'])
    assert.equal(b.state.pending, 'claude auth login')
    assert.match(String(s.getState().shells.claude.pasteError ?? ''), /pending input|busy/i)
  })
})
await test('R77 a refused clipboard paste is visible in the terminal (not a silent discard)', async () => {
  const invoke = async (c: string) => { if (c === 'pty_paste') throw 'The panel is not accepting input right now' }
  await withTerminal(invoke, async t => {
    await t.paste('whatever');
    assert.ok(t.displayed.some(s => s.includes('not accepting input')), `screen: ${JSON.stringify(t.displayed)}`)
  })
})
await test('R77 unsafe clipboard (newline / control / bracketed paste) is refused visibly and never sent', async () => {
  const b = fakeBackend()
  await withTerminal(b.invoke, async t => {
    for (const bad of ['echo a\nrm x', 'echo a\r', 'x\x03', '\x1b[200~y']) await t.paste(bad)
    assert.equal(b.calls.length, 0, 'unsafe clipboard text reached the backend')
    assert.equal(t.displayed.filter(s => s.includes('paste refused')).length, 4)
  })
})

try { rmSync(work, { recursive: true, force: true }) } catch { /* best effort */ }
const failed = results.filter(r => !r.ok)
console.log(`\n${results.length - failed.length}/${results.length} passed; ${failed.length} failed`)
for (const f of failed) console.log(`  FAILED: ${f.name}`)
process.exit(failed.length ? 1 : 0)
