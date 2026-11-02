// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import fixture from './wire.fixture.json' with { type: 'json' }
import type { AppInfo, DecisionResult, Detection, ExportResult, RunEvent, RestartInput, RunSnapshot, RunSummary, Settings, Shell, StagedFile, StartInput } from './types.ts'
import { toPolicy, viewPolicy } from './policy.ts'
// Runtime schemas below validate the imported JSON before adapter use.
const wire: { decisionResult: DecisionResult; runSnapshot: RunSnapshot; restartInput: RestartInput; runSummary: RunSummary; detection: Detection; settings: Settings; shell: Shell; stagedFile: StagedFile; exportResult: ExportResult; startInput: StartInput; appInfo: AppInfo; events: RunEvent[] } = fixture as unknown as typeof wire
type Contract<T> = NonNullable<T> extends readonly (infer U)[] ? readonly Contract<U>[] : NonNullable<T> extends object ? { [K in keyof NonNullable<T>]-?: Contract<NonNullable<T>[K]> } : string
type Schema = string | { [key: string]: Schema } | readonly Schema[]
function assertShape(value: unknown, schema: Schema, path = 'wire'): void {
  if (typeof schema === 'string') {
    const valid = schema.split('|').some(kind => kind === 'null' ? value === null : ['string', 'number', 'boolean'].includes(kind) ? typeof value === kind : value === kind)
    if (!valid) throw new Error(`${path}: expected ${schema}, got ${JSON.stringify(value)}`)
  } else if (Array.isArray(schema)) {
    if (!Array.isArray(value) || !value.length) throw new Error(`${path}: expected populated array`)
    value.forEach((entry, i) => assertShape(entry, schema[0], `${path}[${i}]`))
  } else {
    if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(`${path}: expected object`)
    for (const [key, shape] of Object.entries(schema)) {
      if (!Object.hasOwn(value, key) && typeof shape === 'string' && shape.split('|').includes('undefined')) continue
      if (!Object.hasOwn(value, key)) throw new Error(`${path}.${key}: missing`)
      assertShape((value as Record<string, unknown>)[key], shape, `${path}.${key}`)
    }
    for (const key of Object.keys(value)) if (!Object.hasOwn(schema, key)) throw new Error(`${path}.${key}: unexpected`)
  }
}
if (wire.decisionResult.publication !== 'incomplete' || wire.decisionResult.decision?.kind !== 'returned') throw Error('Committed decision publication wire contract')
const provider = 'claude|codex'
const state = 'running|paused|stopped|completed|approved|rejected|returned'
const stop = 'gate_no_new_issues|gate_clean|gate_max_rounds|gate_human_review_required|user_stop|provider_error|cancelled|null'
const counts = { critical: 'number', high: 'number', medium: 'number', low: 'number' }
const gate = { kind: 'continue|stop|stopped', reason: 'continue_open|continue_min|continue_new|continue_contribution|stop_no_new|stop_clean|stop_max|stopped_human', counts }
const policy = { max_rounds: 'number', min_rounds: 'number', contribution: 'verify|correct|improve', new_content_limit_pct: 'number', stop: { no_critical: 'boolean', no_high: 'boolean', no_new: 'boolean', max_rounds: 'boolean' } }
const shell = { claude: 'string|null', codex: 'string|null' }
const usage = { input_tokens: 'number', output_tokens: 'number', cache_read_tokens: 'number', cache_creation_tokens: 'number|null' }
const decision = { kind: 'approved|rejected|returned', name: 'string', note: 'string', at: 'string', after_round: 'number', revision: 'number', document_sha256: 'string' }
const severity = 'critical|high|medium|low'
assertShape(wire.runSnapshot, {
  generation: 'number', revision: 'number', id: 'string', name: 'string', createdAt: 'string', author: provider, reviewer: provider, policy,
  objective: 'string', criteria: ['string'], state, stopReason: stop, phase: 'author|reviewer|null', pendingRevision: 'number|null',
  lastError: { provider: provider + '|null', kind: 'string', message: 'string' }, roundsDone: 'number', documents: ['string'],
  rounds: [{ round: 'number', gate, newTextPct: 'number', contributionExceeded: 'boolean', authorUsage: usage, reviewerUsage: usage, authorModel: 'string|null', reviewerModel: 'string|null' }],
  issues: [{ issue: { issue_id: 'string', severity, criterion: 'string', finding: 'string', evidence: 'string', proposed_action: 'string', human_decision_required: 'boolean', status: 'open|resolved' }, raisedIn: 'number', resolvedOnArrival: 'number|undefined', history: [{ round: 'number', status: 'open|resolved', severity, finding: 'string' }], authorResponses: [{ round: 'number', response: 'accepted|rejected_with_evidence|partially_accepted|human_decision_required', explanation: 'string', evidence: 'string' }], humanDecision: { issue_id: 'string', choice: 'accept|keep', note: 'string', round: 'number' } }],
  humanQuestions: [{ issue_id: 'string', question: 'string', round: 'number', resolved: 'boolean', answer: 'string|null', answeredAt: 'string|null', answeredBy: 'string|null' }],
  counts, decision, decisionLog: [decision], outputTree: ['string'], primaryName: 'string', supportingNames: ['string'],
} satisfies Contract<RunSnapshot>)
// Both serde's omitted None and a populated arrival round must satisfy the issue contract.
const arrivalShape = { resolvedOnArrival: 'number|undefined' }
assertShape({}, arrivalShape)
assertShape({ resolvedOnArrival: 3 }, arrivalShape)
let rejectedArrival = false
try { assertShape({ resolvedOnArrival: '3' }, arrivalShape) } catch { rejectedArrival = true }
if (!rejectedArrival) throw new Error('Arrival round must be numeric')
assertShape(wire.runSummary, { id: 'string', name: 'string', state, roundsDone: 'number', createdAt: 'string', runDir: 'string' })
assertShape(wire.detection, { provider, installed: 'boolean', version: 'string|null', connected: 'boolean|null', method: 'subscription|api_key|other|null', status_exit_code: 'number|null', error: 'string|null' })
assertShape(wire.settings, { workspace: 'string', shell, author: provider, policy })
assertShape(wire.shell, shell)
assertShape(wire.startInput, { name: 'string', objective: 'string', criteria: ['string'], policy, author: provider, primaryToken: 'string', supportingTokens: ['string'] })
assertShape(wire.restartInput, { primaryToken: 'string', primaryName: 'string', supportingTokens: ['string'], supportingNames: ['string'] } satisfies Contract<RestartInput>)
assertShape(wire.stagedFile, { token: 'string', name: 'string', size: 'number', kind: 'string', chars: 'number', error: 'string|null' })
assertShape(wire.exportResult, { path: 'string', tree: ['string'] })
assertShape(wire.appInfo, { version: 'string', os: 'string', arch: 'string' })
const eventShapes = {
  line: { kind: 'line', provider, role: 'author|reviewer', at: 'string', summary: 'string', raw: 'string' },
  phase: { kind: 'phase', provider, role: 'author|reviewer', round: 'number', phase: 'string' },
  roundCompleted: { kind: 'roundCompleted', round: 'number', gate, newTextPct: 'number', contributionExceeded: 'boolean' },
  state: { kind: 'state', state, stopReason: stop },
  providerError: { kind: 'providerError', provider, role: 'author|reviewer', errorKind: 'string', message: 'string' },
}
if (wire.events.length !== Object.keys(eventShapes).length || new Set(wire.events.map(e => e.kind)).size !== wire.events.length) throw new Error('Missing event variant')
wire.events.forEach(event => assertShape(event, eventShapes[event.kind]))
if (JSON.stringify(toPolicy(viewPolicy(wire.settings.policy))) !== JSON.stringify(wire.settings.policy)) {
  // Compare by keys: serde_json sorts object keys, while JS preserves insertion order.
  const restored = toPolicy(viewPolicy(wire.settings.policy))
  for (const key of ['max_rounds', 'min_rounds', 'contribution', 'new_content_limit_pct'] as const) if (restored[key] !== wire.settings.policy[key]) throw new Error(`Policy round trip: ${key}`)
  for (const key of ['no_critical', 'no_high', 'no_new', 'max_rounds'] as const) if (restored.stop[key] !== wire.settings.policy.stop[key]) throw new Error(`Policy stop: ${key}`)
}
console.log('Wire check passed: 11 fixture entries, 5 event variants and all nested adapter fields')
