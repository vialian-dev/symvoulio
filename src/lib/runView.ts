// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { wasStopped } from './runState'
import { DOC_VERSIONS, ISSUES, authorSummary, docAt, type Block, type IssueDef, type IssueStep } from '../data/sample'
import { gateAfter, issuesAt, type Gate, type HumanIssueDecision, type IssueSnap } from './review'
import type { Run } from '../state/store'
import type { BackendGate, RunSnapshot } from '../services/types'
export function markdownBlocks(markdown: string): Block[] {
  const blocks: Block[] = []
  let paragraph: string[] = []
  const flush = () => { if (paragraph.length) blocks.push({ kind: 'p', text: paragraph.join('\n') }); paragraph = [] }
  for (const line of markdown.split(/\r?\n/)) {
    const heading = /^\s*#+\s+(.*)$/.exec(line)
    if (heading) { flush(); blocks.push({ kind: 'h', text: heading[1] }) }
    else if (!line.trim()) flush()
    else paragraph.push(line)
  }
  flush()
  return blocks
}
const gateKeys: Record<BackendGate['reason'], Gate['key']> = { continue_contribution: 'continueContribution', continue_open: 'continueOpen', continue_min: 'continueMin', continue_new: 'continueNew', stop_no_new: 'stopNoNew', stop_clean: 'stopClean', stop_max: 'stopMax', stopped_human: 'stoppedHuman' }
const EMPTY_DOCUMENT: Block[] = []
export function runView(run: Run, snapshot?: RunSnapshot, desktop = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
  // Canned (browser stand, sample review): exactly the calls the screens made before the adapter existed.
  if (run.sample || (!desktop && !snapshot)) return {
    stopped: wasStopped(run, snapshot, desktop),
    documents: DOC_VERSIONS,
    doc: (rev: number): Block[] => docAt(rev),
    issueDef: (id: string): IssueDef | undefined => ISSUES.find((d) => d.id === id),
    issues: (round: number, decisions: Record<string, HumanIssueDecision> = run.issueDecisions) => issuesAt(round, decisions),
    gate: (round: number): Gate | null => round > 0 ? gateAfter(round, run.policy) : null,
    authorSummary,
  }
  if (!snapshot) return { stopped: wasStopped(run, snapshot, desktop), documents: [] as Block[][], doc: (_rev: number): Block[] => EMPTY_DOCUMENT, issueDef: (_id: string): IssueDef | undefined => undefined, issues: (_round: number, _decisions = run.issueDecisions): IssueSnap[] => [], gate: (_round: number): Gate | null => null, authorSummary: (_round: number) => ({ changes: '', accepted: 0, rejected: 0, partial: 0, human: 0 }) }
  const documents = snapshot.documents.map(markdownBlocks)
  const latest = (id: string) => snapshot.issues.find((i) => i.issue.issue_id === id)
  return {
    stopped: wasStopped(run, snapshot, desktop),
    documents,
    doc: (rev: number): Block[] => documents[Math.min(Math.max(rev, 0), documents.length - 1)] ?? EMPTY_DOCUMENT,
    issueDef(id: string): IssueDef | undefined {
      const i = latest(id)
      if (!i) return undefined
      const steps: Record<number, IssueStep> = {}
      for (const h of i.history) steps[h.round] = { open: h.status === 'open', note: h.finding }
      for (const r of i.authorResponses) steps[r.round] = { ...(steps[r.round] ?? { open: true, note: '' }), response: { kind: r.response, text: r.explanation + (r.evidence ? '\n' + r.evidence : '') } }
      return { id, severity: i.issue.severity, criterion: i.issue.criterion, finding: i.issue.finding, evidence: i.issue.evidence, action: i.issue.proposed_action, humanDecision: i.issue.human_decision_required, raisedIn: i.raisedIn, steps }
    },
    issues(round: number, decisions: Record<string, HumanIssueDecision> = run.issueDecisions): IssueSnap[] {
      return snapshot.issues.filter(i => i.raisedIn <= round).map(i => {
        const history = i.history.filter(h => h.round <= round).sort((a, b) => a.round - b.round)
        const current = history.at(-1)
        const reply = i.authorResponses.find(r => r.round === round)
        const def: IssueDef = { id: i.issue.issue_id, severity: current?.severity ?? i.issue.severity, criterion: i.issue.criterion, finding: current?.finding ?? i.issue.finding, evidence: i.issue.evidence, action: i.issue.proposed_action, humanDecision: i.issue.human_decision_required, raisedIn: i.raisedIn, steps: {} }
        const step: IssueStep = { open: current ? current.status === 'open' : i.issue.status === 'open', note: current?.finding ?? i.issue.finding, response: reply ? { kind: reply.response, text: reply.explanation + (reply.evidence ? '\n' + reply.evidence : '') } : undefined }
        def.steps[round] = step
        const human = decisions[def.id] ?? i.humanDecision ?? undefined
        const decided = human && human.round <= round ? human : undefined
        return { resolvedOnArrival: i.resolvedOnArrival, def, step, open: step.open && decided?.choice !== 'keep', decided }
      })
    },
    gate(round: number): Gate | null {
      const gate = snapshot.rounds.find(r => r.round === round)?.gate
      return gate ? { kind: gate.kind, key: gateKeys[gate.reason], counts: gate.counts } : null
    },
    authorSummary(round: number) {
      const replies = snapshot.issues.flatMap(i => i.authorResponses.filter(r => r.round === round))
      const count = (kind: string) => replies.filter(r => r.response === kind).length
      // RunSnapshot does not expose AuthorResponse.changes_made; use the actual replies.
      return { changes: replies.map(r => r.explanation).filter(Boolean).join('\n'), accepted: count('accepted'), rejected: count('rejected_with_evidence'), partial: count('partially_accepted'), human: count('human_decision_required') }
    },
  }
}
