// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import type { Backend } from './backend'
import type { Policy, RunSnapshot as WireSnapshot, StartInput, RunEvent, StagedFile } from './types'
import { DOC_VERSIONS, ISSUES, issueStep, other } from '../data/sample'
import { openCounts, issuesAt, gateAfter } from '../lib/review'
import type { Run } from '../state/store'
import { toPolicy } from './policy'
type RunSnapshot = Omit<WireSnapshot, 'decision' | 'decisionLog' | 'issues'> & { decision: Run['decision'] | null; decisionLog: Run['decisionLog']; issues: (Omit<WireSnapshot['issues'][number], 'humanDecision'> & { humanDecision: ({ issueId: string } & Run['issueDecisions'][string]) | null })[] }
type CannedBackend = Omit<Backend, 'detect' | 'getRun' | 'startRun' | 'resume' | 'decide' | 'decideIssue' | 'stop'> & { detect(): Promise<{ provider: import('./types').Provider; installed: boolean; version: string; connected: boolean; method: 'subscription' | 'api_key' | null; statusExitCode: null; error: null }[]>; stop(id: string): Promise<RunSnapshot>; getRun: typeof getRun; startRun: typeof startRun; resume(id: string): Promise<RunSnapshot>; decide(id: string, kind: 'approved' | 'rejected' | 'returned', name: string, note: string): Promise<RunSnapshot>; decideIssue(id: string, issueId: string, choice: 'accept' | 'keep', note: string): Promise<RunSnapshot> }
// The existing store owns prototype seeds, shells and timers. Calling through
// this adapter reuses those exact behaviors rather than introducing a second simulation.
const store = () => import('../state/store')
const staged = new Map<string, StagedFile>()
const unavailable = async (): Promise<never> => { throw new Error('Desktop capability is unavailable in the browser') }
export function canned<T>(behavior: () => T): T { return behavior() }
function snapshot(run: Run): RunSnapshot {
  return { ...{ generation: 0, revision: 0, phase: null, pendingRevision: null }, id: run.id, name: run.name, createdAt: run.createdAt, author: run.author, reviewer: other(run.author), policy: toPolicy(run.policy), objective: run.objective, criteria: run.criteria, state: run.state, stopReason: run.stopReason === 'user' ? 'user_stop' : run.stopReason === 'gate' ? 'gate_clean' : null, lastError: run.error ? { provider: run.error.provider, kind: 'auth', message: '' } : null, roundsDone: run.done, documents: DOC_VERSIONS.slice(0, run.done + 1).map(doc => doc.map(b => (b.kind === 'h' ? '# ' : '') + b.text).join('\n\n')), rounds: Array.from({ length: run.done }, (_, n) => { const round = n + 1; const gate = gateAfter(round, run.policy, run.issueDecisions); return { round, gate: { kind: gate.kind, reason: gate.key.replace(/[A-Z]/g, c => '_' + c.toLowerCase()) as RunSnapshot['rounds'][number]['gate']['reason'], counts: gate.counts }, newTextPct: 0, contributionExceeded: false, authorUsage: null, reviewerUsage: null, authorModel: null, reviewerModel: null } }), issues: ISSUES.filter(i => i.raisedIn <= run.done).map(i => ({ issue: { issue_id: i.id, severity: i.severity, criterion: i.criterion, finding: i.finding, evidence: i.evidence, proposed_action: i.action, human_decision_required: i.humanDecision, status: issueStep(i, run.done)?.open ? 'open' : 'resolved' }, raisedIn: i.raisedIn, history: Array.from({ length: run.done - i.raisedIn + 1 }, (_, n) => { const round = n + i.raisedIn; const step = issueStep(i, round)!; return { round, status: step.open ? 'open' as const : 'resolved' as const, severity: i.severity, finding: step.note } }), authorResponses: Object.entries(i.steps).flatMap(([round, step]) => step.response && Number(round) <= run.done ? [{ round: Number(round), response: step.response.kind, explanation: step.response.text, evidence: '' }] : []), humanDecision: run.issueDecisions[i.id] ? { issueId: i.id, ...run.issueDecisions[i.id] } : null })), counts: openCounts(issuesAt(run.done, run.issueDecisions)), decision: run.decision ?? null, decisionLog: run.decisionLog, outputTree: [], primaryName: run.document.name, supportingNames: run.supporting.map(f => f.name), humanQuestions: [] }
}
async function getRun(id: string) {
  const run = (await store()).runById(id)
  if (!run) throw new Error('Run does not exist')
  return snapshot(run)
}
function subscribeRun(runId: string, onEvent: (event: RunEvent) => void): Promise<void> {
  void store().then(s => {
    let previous = s.runById(runId)
    const timer = window.setInterval(() => {
      const run = s.runById(runId)
      if (!run) { window.clearInterval(timer); return }
      if (run.done !== previous?.done) onEvent({ kind: 'roundCompleted', round: run.done, gate: snapshot(run).rounds.at(-1)!.gate, newTextPct: 0, contributionExceeded: false })
      if (run.state !== previous?.state) onEvent({ kind: 'state', state: run.state, stopReason: snapshot(run).stopReason })
      previous = run
      if (!['running', 'paused'].includes(run.state)) window.clearInterval(timer)
    }, 100)
  })
  return Promise.resolve()
}
function policy(p: Policy): Run['policy'] {
  return { maxRounds: p.max_rounds, minRounds: p.min_rounds, contribution: p.contribution, newContent: [0, 10, 25].includes(p.new_content_limit_pct) ? String(p.new_content_limit_pct) as '0' | '10' | '25' : 'custom', customPct: p.new_content_limit_pct, stop: { noCritical: p.stop.no_critical, noHigh: p.stop.no_high, noNew: p.stop.no_new, maxRounds: p.stop.max_rounds } }
}
async function startRun(input: StartInput, onEvent: (event: RunEvent) => void) {
  const s = await store()
  const chip = (token: string) => { const f = staged.get(token); if (!f) throw new Error('Staged file does not exist'); return { name: f.name, size: f.size, type: f.kind } }
  const id = s.startRun({ name: input.name, objective: input.objective, criteria: input.criteria, author: input.author, policy: policy(input.policy), document: chip(input.primaryToken), supporting: input.supportingTokens.map(chip) })
  await subscribeRun(id, onEvent)
  return getRun(id)
}
export const cannedBackend: CannedBackend = {
  detect: async () => { const s = (await store()).getState(); return (['claude', 'codex'] as const).map(provider => ({ provider, ...s.conn[provider], statusExitCode: null, error: null })) },
  paste: async (p, command) => { (await store()).pasteCommand(p, command, command.includes('status') ? 'status' : 'login') },
  ptyOpen: async () => ({ shell: 'pwsh', reattached: false }),
  ptyWrite: async (p, data) => { if (data.includes('\r')) (await store()).pressEnter(p) },
  ptyResize: async () => {},
  stageFile: async (name, bytes) => { const token = `canned-${staged.size}`; const file = { token, name, size: bytes.length, kind: name.split('.').pop() ?? '', chars: new TextDecoder().decode(new Uint8Array(bytes)).length, error: null }; staged.set(token, file); return file },
  startRun, subscribeRun,
  pause: async id => { (await store()).pauseRun(id) },
  resume: async id => { (await store()).resumeRun(id); return getRun(id) },
  stop: async id => { (await store()).stopRun(id); return getRun(id) },
  restart: unavailable, discard: async tokens => { tokens.forEach(token => staged.delete(token)) },
  decide: async (id, kind, name, note) => { (await store()).decide(id, kind, name, note); return { ...await getRun(id), publication: 'complete' as const } },
  decideIssue: async (id, issueId, choice, note) => { (await store()).decideIssue(id, issueId, choice, note); return getRun(id) },
  getRun,
  listRuns: async () => (await store()).getState().runs.map(r => ({ id: r.id, name: r.name, state: r.state, roundsDone: r.done, createdAt: r.createdAt, runDir: '' })),
  deleteRun: async id => { (await store()).deleteRun(id) },
  exportRun: unavailable, openFolder: async () => {},
  appInfo: async () => ({ version: '0.2.0', os: 'browser', arch: 'browser' }),
  getSettings: async () => { const s = (await store()).getState(); return { workspace: s.workspace, shell: s.shellName, author: s.author, policy: toPolicy(s.policy) } },
  setSettings: async settings => { const s = await store(); s.setWorkspace(settings.workspace); s.setAuthor(settings.author); s.setPolicy(policy(settings.policy)); for (const p of ['claude', 'codex'] as const) s.setShellName(p, settings.shell[p] === 'powershell' ? 'powershell' : 'pwsh'); return settings },
}
