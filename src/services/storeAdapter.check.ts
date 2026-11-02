// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Failing before fix: AssertionError [ERR_ASSERTION]: R15 failed starts stay locally failed (paused !== failed)
/// <reference types="node" />
import assert from 'node:assert/strict'
import { desktopBackend } from './backend'
import { connect, action, serviceErrors, runLines } from './storeAdapter'
import { registerFile } from './files'
import { runView } from '../lib/runView'
import { DEFAULT_POLICY } from '../lib/review'
import type { State, Run } from '../state/store'
const run = { id: 'failed-local', document: registerFile({ name: 'x.md', size: 1, type: 'md' }, new Blob(['x'])), supporting: [], policy: DEFAULT_POLICY, issueDecisions: {}, state: 'running' } as unknown as Run
let state = { runs: [run], policy: DEFAULT_POLICY, shellName: {}, conn: {} } as State
Object.assign(desktopBackend, { getSettings: async () => { throw Error('settings unavailable') }, detect: async () => [], listRuns: async () => [], stageFile: async () => { throw Error('CLI missing') } })
connect(() => state, patch => { state = { ...state, ...(typeof patch === 'function' ? patch(state) : patch) } })
state.runs = [run]
await action('startRun', [run.id])
assert(serviceErrors.has(run.id))
assert.equal(state.runs[0].state, 'failed', 'R15 failed starts stay locally failed')
assert.equal(runView(run, undefined, true).documents.length, 0, 'R15 desktop must not show sample')
let deleted = ''
desktopBackend.deleteRun = async id => { deleted = id }
await action('deleteRun', [run.id])
assert.equal(deleted, '', 'uncommitted drafts have no backend run to delete')
assert.equal(state.runs.length, 0)
const previous = { ...state, workspace: 'old' }
state.workspace = 'new'
desktopBackend.setSettings = async () => { throw Error('disk full') }
await action('settings', [previous])
assert.equal(state.workspace, 'old')
assert(serviceErrors.has('settings'))
console.log('R15 rejected backend and empty desktop passed')

// A staged extraction error must release the successful primary and the damaged supporting token.
const released: string[][] = []
state.runs = [{ ...run, supporting: [registerFile({ name: 'broken.docx', size: 1, type: 'docx' }, new Blob(['x']))] }]
desktopBackend.stageFile = async name => ({ token: name, name, size: 1, kind: 'md', chars: 1, error: name.endsWith('.docx') ? 'damaged archive' : null })
desktopBackend.discard = async tokens => { released.push(tokens) }
await action('startRun', [run.id])
assert.deepEqual(released, [['x.md', 'broken.docx']])
assert.equal(state.runs[0].state, 'failed')
console.log('R05 all staged tokens released after extraction failure passed')

// R29: storage events and snapshots must never offer provider reconnection.
const { toPolicy } = await import('./policy')
const snapshot = { generation: 1, revision: 1, id: 'storage-real', name: 'storage', createdAt: '', author: 'claude', reviewer: 'codex', policy: toPolicy(DEFAULT_POLICY), objective: '', criteria: [], state: 'running', stopReason: null, lastError: null, roundsDone: 0, documents: [], rounds: [], issues: [], humanQuestions: [], counts: { critical: 0, high: 0, medium: 0, low: 0 }, decision: null, decisionLog: [], outputTree: [], primaryName: 'x.md', supportingNames: [], phase: null, pendingRevision: null } as import('./types').RunSnapshot
let emit!: (event: import('./types').RunEvent) => void
state.runs = [{ ...run, supporting: [] }]
desktopBackend.startRun = async (_input, onEvent) => { emit = onEvent; return snapshot }
desktopBackend.getRun = async () => snapshot
await action('startRun', [run.id])
emit({ kind: 'providerError', provider: 'claude', role: 'author', errorKind: 'storage', message: 'disk full' })
assert.equal(state.runs[0].state, 'paused')
assert.equal(state.runs[0].error, undefined)
assert.match((serviceErrors.get(run.id) as Error).message, /Storage error: disk full/)
for (const kind of ['storage', 'auth']) {
  desktopBackend.getRun = async () => ({ ...snapshot, state: 'paused', lastError: { kind, provider: 'claude', message: 'failure' } })
  emit({ kind: 'state', state: 'paused', stopReason: null })
  await new Promise(resolve => setTimeout(resolve, 0))
  assert.equal(Boolean(state.runs[0].error), kind === 'auth')
}
state.runs = [{ ...state.runs[0], state: 'running' }]
desktopBackend.getRun = async () => { throw Error('access denied') }
emit({ kind: 'state', state: 'paused', stopReason: null })
await new Promise(resolve => setTimeout(resolve, 0))
assert.equal(state.runs[0].state, 'paused')
assert.equal(state.runs[0].error, undefined)
assert.match((serviceErrors.get(run.id) as Error).message, /Storage error: could not read run state/)
console.log('R29 storage events, snapshots and unreadable run state passed')

// R34 preserves the entered identity on the command; R36 renders controlled progress.
let answerer: string | undefined
desktopBackend.decideIssue = async (_id, _issue, _choice, _note, name) => { answerer = name; return snapshot }
await action('decideIssue', [run.id, 'unique-question', 'accept', 'Finance', 'Dana'])
assert.equal(answerer, 'Dana')
emit({ kind: 'line', provider: 'claude', role: 'author', summary: 'assistant text, 7 words', raw: 'password: secret', at: '' } as import('./types').RunEvent)
assert.equal(runLines.get(run.id)?.claude.at(-1)?.readable, 'assistant text, 7 words')
emit({ kind: 'line', provider: 'claude', role: 'author', summary: 'line', raw: 'password: secret', at: '' })
assert.equal(runLines.get(run.id)?.claude.at(-1)?.readable, 'line', 'legacy provider text is never the safe summary')
console.log('R34 answerer forwarding and R36 controlled live summaries passed')

let detections = 0
desktopBackend.paste = async () => { throw Error('panel busy') }
desktopBackend.detect = async () => { detections++; return [{ provider: 'claude', installed: true, connected: true, version: '1', method: 'subscription', status_exit_code: 0, error: null }] }
assert.equal(await action('checkConnection', ['claude']), true)
assert.equal(detections, 1, 'busy panel still performs connection detection')
assert.equal(state.conn.claude.connected, true)
