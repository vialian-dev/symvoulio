// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
/// <reference types="node" />
import assert from 'node:assert/strict'
import capturedOrder from './storeAdapterRace.trace.json'
import reloadOrder from './storeAdapterReload.trace.json'
import { spawnSync } from 'node:child_process'
import { createRequire } from 'node:module'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { desktopBackend as backend } from './backend'
import { action, connect, refreshRun, snapshots, serviceErrors } from './storeAdapter'
import { registerFile } from './files'
import { DEFAULT_POLICY } from '../lib/review'
import { toPolicy } from './policy'
import type { Run, State } from '../state/store'
import type { RunEvent, RunSnapshot, RunSummary } from './types'
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(r => { resolve = r }); return { promise, resolve } }
const listed = deferred<RunSummary[]>()
const started = deferred<RunSnapshot>()
const entered = deferred<void>()
const running: RunSnapshot & { generation: number; revision: number } = { generation: 1, revision: 1, id: 'backend-race', name: 'race', createdAt: '', author: 'claude', reviewer: 'codex', policy: toPolicy(DEFAULT_POLICY), objective: '', criteria: [], state: 'running', stopReason: null, lastError: null, roundsDone: 0, documents: [], rounds: [], issues: [], humanQuestions: [], counts: { critical: 0, high: 0, medium: 0, low: 0 }, decision: null, decisionLog: [], outputTree: [], primaryName: 'race.md', supportingNames: [], phase: null, pendingRevision: null }
const stopped: RunSnapshot & { generation: number; revision: number } = { ...running, state: 'stopped', generation: 1, revision: 5, roundsDone: 1, stopReason: 'user_stop' }
const local = { id: 'local-race', name: 'race', document: registerFile({ name: 'race.md', size: 1, type: 'md' }, new Blob(['x'])), supporting: [], policy: DEFAULT_POLICY, state: 'running', decisionLog: [], issueDecisions: {} } as unknown as Run
let state = { runs: [local], conn: {}, policy: DEFAULT_POLICY } as State
const savedLinks = new Map<string, string>()
Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: { getItem: (key: string) => savedLinks.get(key) ?? null, setItem: (key: string, value: string) => savedLinks.set(key, value), removeItem: (key: string) => savedLinks.delete(key) } })
Object.assign(backend, { getSettings: async () => ({ workspace: 'replay-workspace', author: 'claude', policy: toPolicy(DEFAULT_POLICY), shell: { claude: null, codex: null } }), detect: async () => [], listRuns: () => listed.promise, stageFile: async () => ({ token: 'token' }), getRun: async () => stopped, startRun: async () => { entered.resolve(); return started.promise } })
connect(() => state, patch => { state = { ...state, ...(typeof patch === 'function' ? patch(state) : patch) } })
assert(state.runs.some(r => r.id === local.id), 'lazy connect must preserve the newly started local run')
const start = action('startRun', [local.id])
await entered.promise
// A read can begin before the start reply binds the local route alias.
const beforeBindingFailure = deferred<RunSnapshot>()
backend.getRun = async () => { await beforeBindingFailure.promise; throw Error('read begun before alias binding failed') }
const oldAliasRead = refreshRun(running.id)
backend.getRun = async () => stopped
// The global run-updated handler runs before run_start returns its backend id.
await refreshRun(running.id)
started.resolve(running)
await start
assert.equal(state.runs.length, 1, 'early backend entry must merge into the local entry')
assert.equal(state.runs[0].id, local.id)
assert.equal(state.runs[0].backendId, running.id)
assert.equal(state.runs[0].state, 'stopped', 'start response cannot undo an early terminal event')
// Resolve a stale request after a newer terminal snapshot.
const stale = deferred<RunSnapshot>()
backend.getRun = () => stale.promise
const refresh = refreshRun(running.id)
stale.resolve(running)
await refresh
listed.resolve([])
await new Promise(resolve => setTimeout(resolve, 0))
assert.equal(state.runs[0].state, 'stopped', 'stale running snapshot must not resurrect a terminal run')
assert.equal(state.runs[0].id, local.id, 'a list missing the new run must preserve its alias')
assert.equal(snapshots.get(local.id), snapshots.get(running.id))
assert.equal(snapshots.get(local.id)?.roundsDone, 1)
console.log('R18b lazy initialization, early run-updated, stale snapshot and missing list passed')

// Recorded at 2026-10-31T11:07:34Z: backend revision 2 precedes start reply 1.
// Hold the subsequent read at the exact point where the recorded alias had no snapshot.
const recorded = capturedOrder.filter(e => e.kind === 'snapshot' && e.decision === 'received')
assert.deepEqual(recorded.map(e => e.revision), [2, 1, 2])
const replayAlias = recorded[1].alias
const replayId = String(recorded[0].backendId)
const replaySnapshot = (e: typeof recorded[number]): RunSnapshot => ({ ...running, id: String(e.backendId), generation: Number(e.generation), revision: Number(e.revision), state: e.state as RunSnapshot['state'], documents: ['# Recorded document'], phase: 'author' })
const replayEarly = replaySnapshot(recorded[0])
const replayReply = replaySnapshot(recorded[1])
const replayFollowUp = replaySnapshot(recorded[2])
const replayLocal = { ...local, id: replayAlias, document: registerFile({ name: 'race.md', size: 1, type: 'md' }, new Blob(['x'])) }
state = { ...state, runs: [replayLocal, ...state.runs] }
const replayEntered = deferred<void>(), replayStartReply = deferred<RunSnapshot>()
let replayEvents!: (e: RunEvent) => void
backend.startRun = async (_input, notify) => { replayEvents = notify; replayEntered.resolve(); return replayStartReply.promise }
const replayStart = action('startRun', [replayAlias])
await replayEntered.promise
const replayReadOne = deferred<RunSnapshot>(), replayReadTwo = deferred<RunSnapshot>()
let replayReadCount = 0
backend.getRun = () => (++replayReadCount === 1 ? replayReadOne.promise : replayReadTwo.promise)
const replayGlobalOne = refreshRun(replayId)
replayEvents({ kind: 'state', state: 'running', stopReason: null })
const replayGlobalTwo = refreshRun(replayId)
replayEvents({ kind: 'phase', provider: 'claude', role: 'author', round: 1, phase: 'author' })
replayReadOne.resolve(replayEarly)
await replayGlobalOne
const replayReadEntered = deferred<void>(), replayHeldRead = deferred<RunSnapshot>()
backend.getRun = () => { replayReadEntered.resolve(); return replayHeldRead.promise }
replayStartReply.resolve(replayReply)
await replayReadEntered.promise
assert.equal(snapshots.get(replayAlias)?.revision, 2, 'R18b recorded order must bind cached revision before the follow-up read')
assert.equal(snapshots.get(replayAlias), snapshots.get(replayId), 'both route identities resolve during the held read')
assert.equal(state.runs.find(r => r.id === replayAlias)?.backendId, replayId)
assert(!state.runs.some(r => r.id === replayId), 'the duplicate backend row was removed')
replayHeldRead.resolve(replayFollowUp)
await replayStart
replayReadTwo.resolve(replayFollowUp)
await replayGlobalTwo
// A storage failure may be the first snapshot observed in a newer generation.
const replayStorage = { ...replayEarly, generation: 2, revision: 3, state: 'paused' as const, lastError: { kind: 'storage', provider: null, message: 'disk full' } }
backend.getRun = async () => replayStorage
await refreshRun(replayId)
backend.getRun = async () => ({ ...replayEarly, generation: 2, revision: 4 })
await refreshRun(replayId)
assert.equal(state.runs.find(r => r.id === replayAlias)?.state, 'paused', 'the incoming failed generation owns the failure barrier')
backend.resume = async () => ({ ...replayEarly, generation: 3, revision: 5 })
await action('resumeRun', [replayAlias])
assert.equal(state.runs.find(r => r.id === replayAlias)?.state, 'running')
state = { ...state, runs: state.runs.filter(r => r.id !== replayAlias) }
backend.getRun = async () => stopped
console.log('R18b recorded alias-loss order and incoming storage-generation barrier passed')
beforeBindingFailure.resolve(stopped)
await oldAliasRead
assert.match(String(serviceErrors.get(local.id)), /read begun before alias binding failed/)
assert(!serviceErrors.has(running.id), 'late read failures must use the bound alias')


// Store the real failing run's identity, then reload the adapter in a fresh process.
const reloadLocal = { ...local, id: reloadOrder.alias, document: registerFile({ name: 'race.md', size: 1, type: 'md' }, new Blob(['x'])) }
state = { ...state, runs: [reloadLocal, ...state.runs] }
const reloadSnapshot = { ...running, id: reloadOrder.backendId, revision: 2 }
backend.startRun = async () => reloadSnapshot
backend.getRun = async () => reloadSnapshot
await action('startRun', [reloadLocal.id])
const savedIdentity = savedLinks.get('symvoulio.runAliases.v2:replay-workspace')
assert(savedIdentity, 'committed backend identities must be saved across context replacement')
assert(JSON.parse(savedIdentity).some((pair: string[]) => pair[0] === reloadOrder.alias && pair[1] === reloadOrder.backendId))
const cli = join(dirname(createRequire(import.meta.url).resolve('jiti/package.json')), 'lib', 'jiti-cli.mjs')
const child = spawnSync(process.execPath, [cli, fileURLToPath(new URL('./storeAdapterReload.check.ts', import.meta.url)), savedIdentity], { encoding: 'utf8', timeout: 30_000 })
assert.equal(child.status, 0, `${child.stdout}\n${child.stderr}`)
backend.deleteRun = async () => {}
await action('deleteRun', [reloadLocal.id])
backend.getRun = async () => stopped
console.log('R18b durable alias write and recorded cold-reload replay passed')



// Every explicit execution owns a new generation; counts cannot order these results.
const current = () => state.runs[0].state
const returned = { ...stopped, generation: 2, revision: 6, state: 'running' as const }
backend.decide = async () => ({ ...returned, publication: 'incomplete' as const })
await action('decide', [local.id, 'returned', 'Dana', 'revise'])
assert.equal(current(), 'running', 'Return applies its own new generation')
backend.getRun = async () => stopped
await refreshRun(running.id)
assert.equal(current(), 'running', 'old Stopped cannot overwrite Return')
const paused = { ...returned, revision: 7, state: 'paused' as const }
backend.pause = async () => {}
backend.getRun = async () => paused
await action('pauseRun', [local.id])
backend.getRun = async () => returned
await refreshRun(running.id)
assert.equal(current(), 'paused', 'old Running cannot overwrite Pause')
const resumed = { ...returned, generation: 3, revision: 8 }
backend.resume = async () => resumed
await action('resumeRun', [local.id])
assert.equal(current(), 'running', 'Resume applies its own generation')
backend.getRun = async () => paused
await refreshRun(running.id)
assert.equal(current(), 'running', 'old Pause cannot overwrite Resume')
const delayed = deferred<RunSnapshot>()
backend.getRun = () => delayed.promise
const inFlight = refreshRun(running.id)
backend.getRun = async () => { throw Error('disk unavailable') }
await refreshRun(running.id)
assert.equal(current(), 'paused')
delayed.resolve(resumed)
await inFlight
assert.equal(current(), 'paused', 'in-flight Running cannot overwrite storage failure')
const failed = { ...resumed, revision: 9, state: 'paused' as const, lastError: { kind: 'storage', provider: null, message: 'disk full' } }
backend.getRun = async () => failed
await refreshRun(running.id)
backend.getRun = async () => resumed
await refreshRun(running.id)
assert.equal(current(), 'paused', 'older persisted Running cannot overwrite storage failure')
// A held read may include progress newer than the last snapshot seen before failure.
backend.getRun = async () => ({ ...resumed, revision: 10 })
await refreshRun(running.id)
assert.equal(current(), 'paused', 'a newer revision in the failed generation cannot restore Running')
backend.resume = async () => ({ ...resumed, generation: 4, revision: 10 })
await action('resumeRun', [local.id])
assert.equal(current(), 'running', 'Resume recovers from storage failure')
console.log('R33 Return, Pause, Resume and storage failure under reordered responses passed')

// An old failed read must not pause an explicit Resume that completed meanwhile.
const oldRead = deferred<RunSnapshot>()
backend.getRun = async () => { await oldRead.promise; throw Error('old read failed') }
const oldFailure = refreshRun(running.id)
backend.resume = async () => ({ ...resumed, generation: 5, revision: 11 })
await action('resumeRun', [local.id])
oldRead.resolve(resumed)
await oldFailure
assert.equal(current(), 'running', 'a failed read from before Resume cannot pause the new execution')
assert.match(String(serviceErrors.get(local.id)), /old read failed/, 'a read failure still surfaces after Resume')
console.log('R33 stale failed read after Resume passed')

// A persisted update within the same execution does not invalidate a read failure.
const coincident = deferred<RunSnapshot>()
backend.getRun = async () => { await coincident.promise; throw Error('disk read failed after event') }
const coincidentFailure = refreshRun(running.id)
backend.getRun = async () => ({ ...resumed, generation: 5, revision: 12 })
await refreshRun(running.id)
coincident.resolve(resumed)
await coincidentFailure
assert.match(String(serviceErrors.get(local.id)), /disk read failed after event/, 'storage failure must surface after a coincident snapshot')
assert.equal(current(), 'paused')
console.log('R33 failed read after coincident snapshot surfaces storage error passed')

// Render the same alert used by S7 for a rejected reused-id round.
const { createElement } = await import('react')
const { renderToStaticMarkup } = await import('react-dom/server')
const { default: i18n } = await import('i18next')
const { initReactI18next } = await import('react-i18next')
const { default: en } = await import('../i18n/en.json')
Object.defineProperty(globalThis, 'location', { configurable: true, value: { search: '' } })
const { createJiti } = await import('jiti')
const { ServiceErrorView, runServiceError } = await createJiti(import.meta.url, { jsx: { runtime: 'automatic' }, fsCache: false }).import<typeof import('./ServiceError')>('./ServiceError')
await i18n.use(initReactI18next).init({ lng: 'en', resources: { en: { translation: en } } })
const rejected = { ...resumed, generation: 5, revision: 13, state: 'paused' as const, lastError: { kind: 'malformed', provider: null, message: 'Question q-reused has different text under an existing id' } }
backend.getRun = async () => rejected
await refreshRun(running.id)
// Exercise the persisted rejection fallback as well as the transient error path.
for (const persistedOnly of [false, true]) {
  if (persistedOnly) serviceErrors.delete(local.id)
  const html = renderToStaticMarkup(createElement(ServiceErrorView, { error: runServiceError(local.id), onDismiss: () => {} }))
  assert.match(html, /role="alert"/, 'R35 backend rejection must render a visible alert')
  assert.match(html, /Question q-reused/, 'R35 alert must name the rejected question')
  assert.doesNotMatch(html, /hidden|display:\s*none/)
}
console.log('R35 reused-id backend rejection renders visible alert naming question passed')

// A committed decision closes the submission flow while keeping the repair warning visible.
const approved = { ...rejected, revision: 14, state: 'approved' as const, lastError: null, decision: { kind: 'approved' as const, name: 'Dana', note: 'saved', at: '', after_round: 1, revision: 1, document_sha256: 'hash' } }
backend.getRun = async () => approved
backend.decide = async () => ({ ...approved, publication: 'incomplete' as const })
assert.equal(await action('decide', [local.id, 'approved', 'Dana', 'saved']), true)
assert.equal(state.runs[0].decision?.name, 'Dana')
assert.equal(state.runs[0].state, 'approved')
const repairAlert = renderToStaticMarkup(createElement(ServiceErrorView, { error: runServiceError(local.id), onDismiss: () => {} }))
assert.match(repairAlert, /Decision saved/)
assert.match(repairAlert, /package will be repaired/)
assert.doesNotMatch(repairAlert, /Operation failed/, 'a committed decision must not be labelled a failed operation')
console.log('R42 committed decision and visible package-repair warning passed')

const traceEntries: Array<Record<string, unknown>> = []
Object.defineProperty(globalThis, 'window', { configurable: true, value: { __symvoulioTrace: traceEntries } })
backend.getRun = async () => stopped
await refreshRun(running.id)
assert.equal(state.runs[0].state, 'approved', 'trace must preserve snapshot ordering behavior')
assert.equal(traceEntries.length, 2, 'received snapshot and ignore decision each have one entry')
assert.equal(traceEntries[0].decision, 'received')
assert.match(String(traceEntries[1].decision), /ignored: older snapshot/)
for (const field of ['time', 'alias', 'backendId', 'generation', 'revision', 'state', 'kind', 'decision']) assert(field in traceEntries[0], `trace includes ${field}`)
delete (globalThis as unknown as { window?: unknown }).window
console.log('R18b optional harness trace records receipt and ordering decision without changing behavior')

// Changing the backend workspace must also change where subsequent bindings persist.
state = { ...state, workspace: 'second-workspace' }
backend.setSettings = async settings => settings
assert.equal(await action('settings', []), true)
const scopedLocal = { ...local, id: 'second-workspace-alias', document: registerFile({ name: 'scope.md', size: 1, type: 'md' }, new Blob(['x'])) }
state = { ...state, runs: [scopedLocal] }
const scopedSnapshot = { ...running, id: 'second-workspace-run' }
backend.startRun = async () => scopedSnapshot
backend.getRun = async () => scopedSnapshot
assert.equal(await action('startRun', [scopedLocal.id]), true)
assert(JSON.parse(savedLinks.get('symvoulio.runAliases.v2:second-workspace')!).some((pair: string[]) => pair[0] === scopedLocal.id))
assert(!JSON.parse(savedLinks.get('symvoulio.runAliases.v2:replay-workspace')!).some((pair: string[]) => pair[0] === scopedLocal.id))
console.log('Alias persistence follows successful backend workspace changes')
