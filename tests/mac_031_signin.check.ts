// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// 0.3.1 F2/F4, desktop branch (review r1): the real storeAdapter «signIn» action against a stand-in backend.
// A busy panel is reset and the login retried; the command is typed and Enter pressed by the app; the connection is
// polled until the provider reports connected; the panel is then reset. A cancelled (stale) attempt never connects or
// resets the panel, even when a newer attempt runs at the same time.
// Run: npx jiti tests/mac_031_signin.check.ts
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { createJiti } from 'jiti'

function load(file: string, dependencies: Record<string, unknown>) {
  const compiler = createJiti(import.meta.url, { fsCache: false })
  const js = compiler.transform({ source: readFileSync(resolve(file), 'utf8'), filename: resolve(file), ts: true })
  const exports: Record<string, any> = {}
  const req = (name: string) => { assert.ok(name in dependencies, `unexpected dependency: ${name}`); return dependencies[name] }
  new Function('require', 'exports', 'module', 'jitiImport', js)(req, exports, { exports }, async (name: string) => req(name))
  return exports
}

type Call = string
function harness(connectedAfter: number, delays: { paste?: number | number[]; reset?: number; detect?: number } = {}, busy = 1) {
  let pastes = 0
  const wait = (ms?: number) => (ms ? new Promise((r) => setTimeout(r, ms)) : Promise.resolve())
  const calls: Call[] = []
  let detects = 0
  let pasteFailures = busy
  const backend = {
    getSettings: async () => ({ workspace: '/ws', author: 'claude', policy: {}, shell: { claude: null, codex: null } }),
    listRuns: async () => [],
    detect: async () => {
      detects++
      if (detects > 1) await wait(delays.detect)
      const connected = detects > connectedAfter
      return [{ provider: 'claude', installed: true, version: '2.1.319', connected, method: connected ? 'subscription' : null }, { provider: 'codex', installed: true, version: '0.167.0', connected: false, method: null }]
    },
    paste: async (panel: string, command: string) => {
      calls.push(`paste ${panel} ${command}`)
      const d = delays.paste
      await wait(Array.isArray(d) ? d[pastes++] : d)
      if (pasteFailures-- > 0) throw new Error('The panel has pending input or is busy.')
    },
    ptyWrite: async (panel: string, data: string) => { calls.push(`write ${panel} ${JSON.stringify(data)}`) },
  }
  const adapter = load('src/services/storeAdapter.ts', {
    './backend': { desktopBackend: backend, onRunSnapshot: async () => () => {}, onRunUpdated: async () => () => {} },
    './runAliases': { loadAliases: () => [], persistAliases: () => {}, pruneAliases: () => {} },
    './files': { fileBytes: async () => [], stagedToken: () => undefined, forgetStaged: () => {} },
    '../lib/runState': { finalHumanGate: () => false },
    '../lib/review': { DEFAULT_POLICY: {} },
    './policy': { toPolicy: (p: unknown) => p, viewPolicy: (p: unknown) => p },
    '../lib/script': { STEPS_PER_SIDE: 3 },
    './panelShells': { resetPanel: async (panel: string) => { calls.push(`reset ${panel}`); await wait(delays.reset); return true } },
  })
  let state: any = { conn: { claude: { connected: false }, codex: { connected: false } }, signingIn: { claude: true, codex: false }, runs: [], workspace: '/ws', shellName: { claude: '', codex: '' } }
  adapter.connect(() => state, (patch: any) => { state = { ...state, ...(typeof patch === 'function' ? patch(state) : patch) } })
  adapter.signInTiming.pollMs = 5
  adapter.signInTiming.waitMs = 5_000
  return { adapter, calls, get state() { return state } }
}

// A: busy panel -> reset -> login typed -> Enter -> polled until connected -> panel reset.
{
  const h = harness(3)
  await new Promise((r) => setTimeout(r, 20))
  const ok = await h.adapter.action('signIn', ['claude', () => true])
  assert.equal(ok, true)
  assert.deepEqual(h.calls, ['paste claude claude auth login', 'reset claude', 'paste claude claude auth login', 'write claude "\\r"', 'reset claude'])
  assert.equal(h.state.conn.claude.connected, true)
}

// B: a cancelled attempt stops polling and leaves the panel alone; a newer attempt still connects and resets once.
{
  const h = harness(4)
  await new Promise((r) => setTimeout(r, 20))
  let first = true
  const stale = h.adapter.action('signIn', ['claude', () => first])
  await new Promise((r) => setTimeout(r, 2))
  first = false // «Cancel»
  const fresh = h.adapter.action('signIn', ['claude', () => true])
  assert.equal(await stale, false)
  assert.equal(await fresh, true)
  assert.equal(h.calls.filter((c) => c === 'reset claude').length, 2, `one reset for the busy panel, one after the newer sign-in: ${h.calls.join(' | ')}`)
  assert.equal(h.calls.filter((c) => c.startsWith('write')).length, 2)
}

// C: a sign-in that never completes gives up at the deadline without resetting the panel.
{
  const h = harness(Number.MAX_SAFE_INTEGER)
  await new Promise((r) => setTimeout(r, 20))
  h.adapter.signInTiming.waitMs = 60
  assert.equal(await h.adapter.action('signIn', ['claude', () => true]), false)
  assert.equal(h.calls.filter((c) => c === 'reset claude').length, 1, h.calls.join(' | '))
}
// Review r2: «Cancel» while an await is pending. D: during the paste -> the typed command is erased, never run.
{
  const h = harness(0, { paste: 60 }, 0)
  await new Promise((r) => setTimeout(r, 20))
  let live = true
  const run = h.adapter.action('signIn', ['claude', () => live])
  setTimeout(() => { live = false }, 10)
  assert.equal(await run, false)
  assert.deepEqual(h.calls, ['paste claude claude auth login', 'write claude "\\u0015"'])
}
// E: during the reset of a busy panel -> no second paste, no Enter.
{
  const h = harness(0, { reset: 60 }, 1)
  await new Promise((r) => setTimeout(r, 20))
  let live = true
  const run = h.adapter.action('signIn', ['claude', () => live])
  setTimeout(() => { live = false }, 10)
  assert.equal(await run, false)
  assert.deepEqual(h.calls, ['paste claude claude auth login', 'reset claude'])
}
// F: during a detection that reports connected -> the stale attempt does not reset the panel.
{
  const h = harness(1, { detect: 60 }, 0)
  await new Promise((r) => setTimeout(r, 20))
  let live = true
  const run = h.adapter.action('signIn', ['claude', () => live])
  setTimeout(() => { live = false }, 30)
  assert.equal(await run, false)
  assert.equal(h.calls.filter((c) => c === 'reset claude').length, 0, h.calls.join(' | '))
  assert.equal(h.calls.filter((c) => c === 'write claude "\\r"').length, 1)
}
// G (review r3): Cancel -> Sign in while BOTH pastes are slow. The stale attempt's Ctrl+U must come before the newer
// attempt types; the newer attempt's command is then entered.
{
  const h = harness(2, { paste: [60, 30] }, 0)
  await new Promise((r) => setTimeout(r, 20))
  let a = true
  const first = h.adapter.action('signIn', ['claude', () => a])
  await new Promise((r) => setTimeout(r, 10))
  a = false // «Cancel»
  const second = h.adapter.action('signIn', ['claude', () => true])
  assert.equal(await first, false)
  assert.equal(await second, true)
  assert.deepEqual(h.calls, ['paste claude claude auth login', 'write claude "\\u0015"', 'paste claude claude auth login', 'write claude "\\r"', 'reset claude'])
}
console.log('mac 0.3.1 sign-in check passed')
