// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Fresh-process replay of the final context in the failed 2026-10-31T11:24Z run.
import assert from 'node:assert/strict'
import recorded from './storeAdapterReload.trace.json'
import { DEFAULT_POLICY } from '../lib/review'
import { toPolicy } from './policy'
import type { RunSnapshot, RunSummary, RunEvent } from './types'
import type { State } from '../state/store'
const key = 'symvoulio.runAliases.v2:replay-workspace'
const initialAliases = JSON.parse(process.argv[2] ?? JSON.stringify([[recorded.alias, recorded.backendId]])) as [string, string][]
initialAliases.push(['externally-deleted-alias', 'externally-deleted-run'])
const stored = new Map<string, string>([[key, JSON.stringify(initialAliases)]])
Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: { getItem: (key: string) => stored.get(key) ?? null, setItem: (key: string, value: string) => stored.set(key, value), removeItem: (key: string) => stored.delete(key) } })
Object.defineProperty(globalThis, 'location', { configurable: true, value: { search: '', pathname: `/review/${recorded.alias}` } })
const { desktopBackend: backend } = await import('./backend')
const { connect, refreshRun, action, snapshots, serviceErrors } = await import('./storeAdapter')
const { matchesRun } = await import('../state/store')
const frames: RunSnapshot[] = recorded.snapshots.map(e => ({ id: recorded.backendId, generation: e.generation, revision: e.revision, state: e.state as RunSnapshot['state'], name: 'reload replay', createdAt: '', author: 'claude', reviewer: 'codex', policy: toPolicy(DEFAULT_POLICY), objective: '', criteria: [], stopReason: e.state === 'stopped' ? 'user_stop' : null, lastError: null, roundsDone: e.revision >= 6 ? 1 : 0, documents: ['# Original', '# Revised'], rounds: [], issues: [], humanQuestions: [], counts: { critical: 0, high: 0, medium: 0, low: 0 }, decision: null, decisionLog: [], outputTree: [], primaryName: 'source.md', supportingNames: [], phase: null, pendingRevision: null }))
let loaded!: () => void, event!: (e: RunEvent) => void
const subscribed = new Promise<void>(resolve => { loaded = resolve })
let next = 0, deleted = ''
const summary: RunSummary = { id: recorded.backendId, name: 'reload replay', state: 'running', roundsDone: 0, createdAt: '', runDir: '' }
Object.assign(backend, { getSettings: async () => ({ workspace: 'replay-workspace', author: 'claude', policy: toPolicy(DEFAULT_POLICY), shell: { claude: null, codex: null } }), detect: async () => [], listRuns: async () => [summary], getRun: async () => frames[next++], subscribeRun: async (_id: string, callback: typeof event) => { event = callback; loaded() }, deleteRun: async (id: string) => { deleted = id } })
let state = { runs: [], conn: {}, policy: DEFAULT_POLICY } as unknown as State
connect(() => state, patch => { state = { ...state, ...(typeof patch === 'function' ? patch(state) : patch) } })
await subscribed
for (let i = 1; i < frames.length; i++) await refreshRun(recorded.backendId)
const route = state.runs.find(run => matchesRun(run, recorded.alias))
assert.equal(route?.state, 'stopped', 'R18b failed trace: stopped backend must resolve the original URL alias after reload')
assert.equal(snapshots.get(recorded.alias)?.revision, 7)
assert.equal(state.runs.length, 1, 'reloaded summaries must not create duplicate alias/backend rows')
event({ kind: 'providerError', provider: 'claude', role: 'author', errorKind: 'storage', message: 'disk unavailable' })
assert(serviceErrors.has(recorded.alias), 'reloaded subscriptions use the canonical alias for errors')
await action('deleteRun', [recorded.alias])
assert.equal(deleted, recorded.backendId)
assert(!JSON.parse(stored.get(key)!).some((pair: string[]) => pair[0] === recorded.alias), 'deleted runs must lose their persisted alias')
console.log('R18b failed reload trace resolves stopped/1 through the original alias and removes deleted aliases')
