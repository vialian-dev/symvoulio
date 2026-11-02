// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// The public, deterministic review gate, and issue snapshots per round. Pure functions: the store and the
// screens read the same numbers.
import { ISSUES, issueStep, type IssueDef, type IssueStep, type Severity } from '../data/sample'

export type Contribution = 'verify' | 'correct' | 'improve'
export type NewContent = '0' | '10' | '25' | 'custom'
export interface Policy {
  maxRounds: number
  minRounds: number
  contribution: Contribution
  newContent: NewContent
  customPct: number
  stop: { noCritical: boolean; noHigh: boolean; noNew: boolean; maxRounds: boolean }
}
export const DEFAULT_POLICY: Policy = {
  maxRounds: 5, minRounds: 2, contribution: 'correct', newContent: '10', customPct: 15,
  stop: { noCritical: true, noHigh: true, noNew: true, maxRounds: true },
}

export interface HumanIssueDecision { choice: 'accept' | 'keep'; note: string; round: number }

export interface IssueSnap { def: IssueDef; step: IssueStep; open: boolean; decided?: HumanIssueDecision; resolvedOnArrival?: number }

/** Issues as the review of `round` left them. Only an explicit Keep waiver closes an issue before the reviewer resolves it. */
export function issuesAt(round: number, decisions: Record<string, HumanIssueDecision> = {}): IssueSnap[] {
  if (round < 1) return []
  const out: IssueSnap[] = []
  for (const def of ISSUES) {
    const step = issueStep(def, round)
    if (!step) continue
    const decided = decisions[def.id]
    out.push({ def, step, open: step.open && decided?.choice !== 'keep', decided })
  }
  return out
}

export type Counts = Record<Severity, number>
export function openCounts(snaps: IssueSnap[]): Counts {
  const c: Counts = { critical: 0, high: 0, medium: 0, low: 0 }
  for (const s of snaps) if (s.open) c[s.def.severity]++
  return c
}
const newIn = (round: number) => ISSUES.filter((d) => d.raisedIn === round).length

export type GateKind = 'continue' | 'stop' | 'stopped'
export interface Gate { kind: GateKind; key: 'continueContribution' | 'continueOpen' | 'continueMin' | 'continueNew' | 'stopNoNew' | 'stopClean' | 'stopMax' | 'stoppedHuman'; counts: Counts }

/** Gate after the review of `round`. */
export function gateAfter(round: number, policy: Policy, decisions: Record<string, HumanIssueDecision> = {}): Gate {
  const counts = openCounts(issuesAt(round, decisions))
  const blocking = (policy.stop.noCritical && counts.critical > 0) || (policy.stop.noHigh && counts.high > 0)
  const atMax = round >= policy.maxRounds
  if (atMax) return blocking || counts.critical + counts.high > 0 ? { kind: 'stopped', key: 'stoppedHuman', counts } : { kind: 'stop', key: 'stopMax', counts }
  if (blocking) return { kind: 'continue', key: 'continueOpen', counts }
  if (round < policy.minRounds) return { kind: 'continue', key: 'continueMin', counts }
  if (policy.stop.noNew) {
    if (round >= 2 && newIn(round) === 0 && newIn(round - 1) === 0) return { kind: 'stop', key: 'stopNoNew', counts }
    return { kind: 'continue', key: 'continueNew', counts }
  }
  return { kind: 'stop', key: 'stopClean', counts }
}
