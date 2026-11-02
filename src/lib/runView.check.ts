// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { markdownBlocks, runView } from './runView.ts'
import { DOC_VERSIONS, authorSummary } from '../data/sample.ts'
import { DEFAULT_POLICY, gateAfter, issuesAt } from './review.ts'
import type { Run } from '../state/store.ts'
function check(condition: boolean) { if (!condition) throw new Error('runView check failed') }
check(JSON.stringify(markdownBlocks('# Title\n\nFirst\nsecond\n\n## Next')) === JSON.stringify([{ kind: 'h', text: 'Title' }, { kind: 'p', text: 'First\nsecond' }, { kind: 'h', text: 'Next' }]))
const run = { policy: DEFAULT_POLICY, issueDecisions: {} } as Run
const view = runView(run)
check(view.documents === DOC_VERSIONS)
for (let round = 1; round <= 4; round++) {
  check(JSON.stringify(view.issues(round, {})) === JSON.stringify(issuesAt(round)))
  check(JSON.stringify(view.gate(round)) === JSON.stringify(gateAfter(round, DEFAULT_POLICY)))
  check(JSON.stringify(view.authorSummary(round)) === JSON.stringify(authorSummary(round)))
}

// Desktop history, reply vocabulary, gate reasons and plain IPC.
const { toPolicy } = await import('../services/policy.ts')
const desktop: import('../services/types.ts').RunSnapshot = {
  ...{ generation: 0, revision: 0, phase: null, pendingRevision: null },
  id: 'desktop-check', name: 'Check', createdAt: '2026-10-30T00:00:00Z', author: 'claude', reviewer: 'codex', policy: toPolicy(DEFAULT_POLICY), objective: '', criteria: [], state: 'completed', stopReason: 'gate_clean', lastError: null, roundsDone: 2,
  documents: ['# Original\n\nText', '# Revised\n\nChanged'],
  rounds: [{ round: 2, gate: { kind: 'stop', reason: 'stop_clean', counts: { critical: 0, high: 0, medium: 0, low: 0 } }, newTextPct: 10, contributionExceeded: false, authorUsage: null, reviewerUsage: null, authorModel: null, reviewerModel: null }],
  issues: [{ issue: { issue_id: 'I-1', severity: 'high', criterion: 'Accuracy', finding: 'Fixed', evidence: 'Source', proposed_action: 'Correct', human_decision_required: false, status: 'resolved' }, raisedIn: 1, history: [{ round: 1, status: 'open', severity: 'critical', finding: 'Incorrect' }, { round: 2, status: 'resolved', severity: 'high', finding: 'Fixed' }], authorResponses: [{ round: 2, response: 'rejected_with_evidence', explanation: 'Explanation', evidence: 'Source' }], humanDecision: { issue_id: 'I-1', choice: 'keep', note: 'Known risk', round: 2 } }],
  counts: { critical: 0, high: 0, medium: 0, low: 0 }, decision: null, decisionLog: [], outputTree: ['final/final_document.md'], primaryName: 'input.md', supportingNames: [], humanQuestions: [],
}
// React effect dependencies must remain stable while the first desktop snapshot is pending.
for (const pending of [runView(run, undefined, true), runView(run, { ...desktop, documents: [] }, true)]) {
  for (const rev of [-1, 0, 1]) check(pending.doc(rev) === pending.doc(rev))
}
console.log('R18b pending desktop document references are stable')
// Arrival metadata survives IPC mapping without changing closure or canned data.
const arrival = { ...desktop.issues[0], resolvedOnArrival: 1, history: [{ round: 1, status: 'resolved' as const, severity: 'high' as const, finding: 'Already fixed' }] }
const arrivalView = runView(run, { ...desktop, issues: [arrival] }, true)
check(arrivalView.issues(0).length === 0)
check(arrivalView.issues(1)[0].resolvedOnArrival === 1 && !arrivalView.issues(1)[0].open)
check(runView(run, desktop, true).issues(1)[0].resolvedOnArrival === undefined)
console.log('P5b arrival round preserved and issue closed; ordinary issues unmarked')
const realView = runView(run, desktop)
check(realView.documents[0][0].text === 'Original')
check(realView.issues(0).length === 0)
check(realView.issues(1)[0].open && realView.issues(1)[0].def.severity === 'critical')
check(!realView.issues(2)[0].open && realView.issues(2)[0].step.response?.kind === 'rejected_with_evidence')
check(realView.gate(1) === null && realView.gate(2)?.key === 'stopClean')
check(realView.authorSummary(2).rejected === 1)
for (const [reason, key] of Object.entries({ continue_open: 'continueOpen', continue_min: 'continueMin', continue_new: 'continueNew', stop_no_new: 'stopNoNew', stop_clean: 'stopClean', stop_max: 'stopMax', stopped_human: 'stoppedHuman' })) {
  desktop.rounds[0].gate.reason = reason as typeof desktop.rounds[number]['gate']['reason']
  check(realView.gate(2)?.key === key)
}
check(toPolicy({ ...DEFAULT_POLICY, newContent: 'custom', customPct: 37 }).new_content_limit_pct === 37)
for (const newContent of ['0', '10', '25'] as const) check(toPolicy({ ...DEFAULT_POLICY, newContent }).new_content_limit_pct === Number(newContent))
let command = ''
let args: Record<string, unknown> = {}
Object.defineProperty(globalThis, 'window', { configurable: true, value: { __TAURI_INTERNALS__: {
  transformCallback: () => 1,
  invoke: async (name: string, input: Record<string, unknown>) => { command = name; args = input; return desktop },
} } })
const { desktopBackend, backend, isDesktop } = await import('../services/backend.ts')
check(isDesktop && backend === desktopBackend)
check(await desktopBackend.getRun(desktop.id) === desktop)
check(command === 'run_get' && args.runId === desktop.id)
let event: import('../services/types.ts').RunEvent | undefined
await desktopBackend.startRun({ name: 'Check', objective: '', criteria: [], policy: toPolicy(DEFAULT_POLICY), author: 'claude', primaryToken: 'primary', supportingTokens: [] }, e => { event = e })
check(command === 'run_start' && Boolean(args.onEvent))
const incoming: import('../services/types.ts').RunEvent = { kind: 'state', state: 'stopped', stopReason: 'user_stop' }
;(args.onEvent as { onmessage: (value: unknown) => void }).onmessage(incoming)
check(event === incoming)
check(event?.kind === 'state' && event.stopReason === 'user_stop')
await desktopBackend.paste('claude', 'claude auth status')
check(command === 'pty_paste' && args.command === 'claude auth status')

const fixture = (await import('../services/wire.fixture.json', { with: { type: 'json' } })).default
const golden = fixture.runSnapshot as import('../services/types.ts').RunSnapshot
const goldenView = runView(run, golden)
check(goldenView.issues(1)[0].def.id === 'I-1')
check(goldenView.issues(1)[0].decided?.choice === 'keep')
check(goldenView.authorSummary(1).rejected === 1)
check(goldenView.gate(1)?.key === 'stoppedHuman')

// doc(rev) and issueDef(id): canned = the sample accessors the screens used; desktop = clamped snapshot documents.
const { docAt, ISSUES } = await import('../data/sample.ts')
for (const rev of [0, 2, 9]) check(JSON.stringify(view.doc(rev)) === JSON.stringify(docAt(rev)))
check(view.issueDef('R01-002') === ISSUES.find((d) => d.id === 'R01-002'))
check(realView.doc(5)[0].text === 'Revised' && realView.doc(-1)[0].text === 'Original')
const def = realView.issueDef('I-1')
check(def !== undefined && def.raisedIn === 1 && def.steps[1].open && !def.steps[2].open && def.steps[2].response?.kind === 'rejected_with_evidence')
console.log('runView check passed')

// Accept requests implementation; Keep alone waives an open finding.
desktop.issues[0].issue.status = 'open'
desktop.issues[0].history[1].status = 'open'
const accepted = { 'I-1': { choice: 'accept' as const, note: 'implement', round: 2 } }
check(runView(run, desktop).issues(2, accepted)[0].open)
check(!runView(run, desktop).issues(2, { 'I-1': { ...accepted['I-1'], choice: 'keep' } })[0].open)
console.log('R09 accept remains open and keep waives passed')
