// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Stub data of the clickable prototype: the sample business case with deliberate gaps (an unsourced
// forecast and totals that do not reconcile), the 5 sample criteria, the canned 3-round review and the
// seeded runs of S11. Content is data, not UI strings: it is the document and the findings, not the interface.

export type Provider = 'claude' | 'codex'
export const PROVIDERS: Provider[] = ['claude', 'codex']
export const other = (p: Provider): Provider => (p === 'claude' ? 'codex' : 'claude')

export type Severity = 'critical' | 'high' | 'medium' | 'low'
export const SEVERITIES: Severity[] = ['critical', 'high', 'medium', 'low']
export type ResponseKind = 'accepted' | 'rejected_with_evidence' | 'partially_accepted' | 'human_decision_required'

export interface Block { kind: 'h' | 'p'; text: string }

const T = 'Northwind Logistics: Midwest Hub Expansion — Business Case'

const summary0 =
  'Northwind Logistics proposes to open a cross-dock hub in Columbus, Ohio, to serve customers in Ohio, Indiana and Michigan. The hub replaces two leased depots whose leases end in 2027 and lets us consolidate less-than-truckload shipments that today travel through Chicago. We have looked carefully at a range of options over the past year and we believe this proposal represents the best balance between cost, service and risk for the company. The total capital cost is $4.2M. Shipment volume through the hub is expected to grow 18 % per year, and the investment pays back in 2.1 years.'
const summary1 =
  'Northwind Logistics proposes a cross-dock hub in Columbus, Ohio, serving Ohio, Indiana and Michigan. It replaces two leased depots whose leases end in 2027 and consolidates less-than-truckload shipments that now travel through Chicago. We considered three options over the past year; this one balances cost, service and risk best. Total capital cost is $4.2M. Shipment volume is expected to grow 18 % per year, and the investment pays back in 2.1 years.'
const summary2 =
  'A cross-dock hub in Columbus, Ohio, replaces two depots whose leases end in 2027 and consolidates shipments that now travel through Chicago. Total capital cost is $4.6M. Volume is expected to grow 18 % per year (our extrapolation of the 2026 Ohio Regional Freight Study); payback is 2.1 years including lease savings.'
const summary3 =
  'A cross-dock hub in Columbus, Ohio, replaces two depots whose leases end in 2027 and consolidates shipments that now travel through Chicago. Total capital cost is $4.6M. Volume is expected to grow 11 % per year (2026 Ohio Regional Freight Study, Table 4); payback is 2.3 years including lease savings of $0.5M per year.'

const forecast0 = '3.2 We expect shipment volume through the hub to grow 18 % per year from 2027 to 2031, driven by e-commerce growth across the three states.'
const forecast2 = '3.2 We expect shipment volume through the hub to grow 18 % per year from 2027 to 2031. The figure is our extrapolation of the 2026 Ohio Regional Freight Study.'
const forecast3 = '3.2 We expect shipment volume through the hub to grow 11 % per year from 2027 to 2031 (2026 Ohio Regional Freight Study, Table 4).'

const capex = 'Table 2. Land and building $2.9M · Racking and dock equipment $1.1M · IT and systems $0.6M · Total $4.6M.'
const cash0 = 'Table 3. Net annual cash flow: Year 1 $1.2M · Year 2 $1.7M · Year 3 $2.0M. Payback period: 2.1 years.'
const cash3 = 'Table 3. Net annual cash flow including lease savings of $0.5M per year: Year 1 $1.7M · Year 2 $2.2M · Year 3 $2.5M. Payback period: 2.3 years.'
const risk0 = 'Labour availability: the Columbus warehouse labour market is tight. Mitigation: wage review in Q2 2027.'
const risk2 = 'Labour availability: the Columbus warehouse labour market is tight. Mitigation: wage review and a temporary-staff framework agreement in Q2 2027; owner to be confirmed by Operations.'
const risk4 = 'Labour availability: the Columbus warehouse labour market is tight. Mitigation: wage review and a temporary-staff framework agreement by 30 June 2027; owner: Director of Operations.'

function doc(summary: string, forecast: string, cash: string, risk: string): Block[] {
  return [
    { kind: 'h', text: T },
    { kind: 'h', text: 'Executive summary' },
    { kind: 'p', text: summary },
    { kind: 'h', text: '1. Background' },
    { kind: 'p', text: 'Northwind moves 2,400 less-than-truckload shipments a week in the Midwest. About 40 % of them pass through the Chicago terminal even when origin and destination are both east of Indianapolis, which adds a day to transit and $38 per shipment in linehaul cost.' },
    { kind: 'h', text: '2. Market' },
    { kind: 'p', text: '2.1 The Midwest less-than-truckload market was worth $9.8B (Freight Analytics, 2019). Our share is 1.6 %.' },
    { kind: 'h', text: '3. Demand forecast' },
    { kind: 'p', text: forecast },
    { kind: 'h', text: '4. Capital cost' },
    { kind: 'p', text: capex },
    { kind: 'p', text: '4.3 The two depot leases that end in 2027 cost $0.5M per year together; the hub removes them.' },
    { kind: 'h', text: '5. Financial return' },
    { kind: 'p', text: cash },
    { kind: 'h', text: '6. Risks' },
    { kind: 'p', text: risk },
  ]
}

/** Index 0 = original, k = revision k. Revisions after 4 repeat revision 4. */
export const DOC_VERSIONS: Block[][] = [
  doc(summary0, forecast0, cash0, risk0),
  doc(summary1, forecast0, cash0, risk0),
  doc(summary2, forecast2, cash0, risk2),
  doc(summary3, forecast3, cash3, risk2),
  doc(summary3, forecast3, cash3, risk4),
]
export const docAt = (rev: number) => DOC_VERSIONS[Math.min(Math.max(rev, 0), DOC_VERSIONS.length - 1)]

export const SAMPLE_DOC = { name: 'business_case_northwind.md', size: 6_214, type: 'md' }
export const SAMPLE_SUPPORTING = [
  { name: 'ohio_regional_freight_study_2026.pdf', size: 1_482_113, type: 'pdf' },
  { name: 'capex_estimate.csv', size: 2_048, type: 'csv' },
]
export const SAMPLE_OBJECTIVE = 'Improve this document for executive review without adding unsupported claims.'
/** The five sample criteria. */
export const SAMPLE_CRITERIA = [
  'All factual claims must be supported.',
  'All totals must reconcile.',
  'No unresolved Critical risks.',
  'Maximum length: 8 pages.',
  'Tone: executive and concise.',
]

/** S6 «Add from template»: three sample templates. */
export const TEMPLATES: { id: string; criteria: string[] }[] = [
  { id: 'business_case', criteria: SAMPLE_CRITERIA },
  { id: 'policy', criteria: ['Every obligation names an owner.', 'Terms are defined before use.', 'No contradiction with the parent policy.', 'Exceptions state who approves them.', 'Plain language, no undefined acronyms.'] },
  { id: 'requirements', criteria: ['Every requirement is testable.', 'Each requirement has a unique ID.', 'No two requirements contradict each other.', 'Non-functional requirements state a number.', 'Out-of-scope items are listed.'] },
]
/** Fixed example criteria for S6; no provider is called. */
export const SUGGESTED_CRITERIA = ['Every forecast names its source and year.', 'The executive summary is under 200 words.']

// ---------- issues of the canned run ----------
export interface IssueStep {
  /** Author response given in this round's revision (to the previous review). */
  response?: { kind: ResponseKind; text: string }
  /** Reviewer's status after this round's review. */
  open: boolean
  note: string
}
export interface IssueDef {
  id: string
  severity: Severity
  criterion: string
  finding: string
  evidence: string
  action: string
  humanDecision: boolean
  raisedIn: number
  /** Keyed by round; later rounds repeat the last defined step. */
  steps: Record<number, IssueStep>
}

export const ISSUES: IssueDef[] = [
  {
    id: 'R01-001', severity: 'critical', criterion: 'All totals must reconcile.',
    finding: 'The capital cost table sums to $4.6M, but the executive summary states $4.2M.',
    evidence: 'Section 4, Table 2 vs. Executive summary, paragraph 1', action: 'Reconcile the table and the summary and state which figure is correct.',
    humanDecision: false, raisedIn: 1,
    steps: {
      1: { open: true, note: 'Raised by the reviewer.' },
      2: { response: { kind: 'accepted', text: 'Corrected the summary to $4.6M; Table 2 was right.' }, open: false, note: 'Resolved: summary and Table 2 agree.' },
    },
  },
  {
    id: 'R01-002', severity: 'high', criterion: 'All factual claims must be supported.',
    finding: 'The 18 % annual volume growth forecast has no source.',
    evidence: 'Section 3.2', action: 'Add the source or remove the claim.',
    humanDecision: false, raisedIn: 1,
    steps: {
      1: { open: true, note: 'Raised by the reviewer.' },
      2: { response: { kind: 'partially_accepted', text: 'Linked the forecast to the 2026 Ohio Regional Freight Study; 18 % is our own extrapolation of it.' }, open: true, note: 'Still open: the study gives 11 %, the extrapolation is not shown.' },
      3: { response: { kind: 'accepted', text: 'Replaced 18 % with the study\'s 11 % and cited Table 4.' }, open: false, note: 'Resolved: the figure matches the cited source.' },
    },
  },
  {
    id: 'R01-003', severity: 'high', criterion: 'All totals must reconcile.',
    finding: 'A payback period of 2.1 years does not follow from Table 3; its cash flows give 2.9 years.',
    evidence: 'Section 5, Table 3', action: 'Recalculate the payback period from Table 3.',
    humanDecision: false, raisedIn: 1,
    steps: {
      1: { open: true, note: 'Raised by the reviewer.' },
      2: { response: { kind: 'rejected_with_evidence', text: 'Table 3 excludes the lease savings of Section 4.3; with them the payback is 2.1 years.' }, open: true, note: 'Still open: the lease savings are in no table, so the figure cannot be checked.' },
      3: { response: { kind: 'accepted', text: 'Added the lease savings to Table 3; payback recalculated as 2.3 years.' }, open: false, note: 'Resolved: 2.3 years follows from Table 3.' },
    },
  },
  {
    id: 'R01-004', severity: 'medium', criterion: 'Tone: executive and concise.',
    finding: 'The executive summary is long and repeats the background.',
    evidence: 'Executive summary', action: 'Cut the summary to the decision, the cost, the forecast and the payback.',
    humanDecision: false, raisedIn: 1,
    steps: {
      1: { open: true, note: 'Raised by the reviewer.' },
      2: { response: { kind: 'accepted', text: 'Shortened the summary to four sentences.' }, open: false, note: 'Resolved.' },
    },
  },
  {
    id: 'R01-005', severity: 'medium', criterion: 'All factual claims must be supported.',
    finding: 'The market size cites a 2019 source.',
    evidence: 'Section 2.1', action: 'Use a current source or mark the figure as dated.',
    humanDecision: true, raisedIn: 1,
    steps: {
      1: { open: true, note: 'Raised by the reviewer.' },
      2: { response: { kind: 'human_decision_required', text: 'No newer public source was found. Keeping the 2019 figure is a business decision.' }, open: true, note: 'Open: needs a human decision.' },
    },
  },
  {
    id: 'R01-006', severity: 'medium', criterion: 'No unresolved Critical risks.',
    finding: 'The labour availability risk has no mitigation owner.',
    evidence: 'Section 6, Risks', action: 'Name an owner and a mitigation date.',
    humanDecision: false, raisedIn: 1,
    steps: {
      1: { open: true, note: 'Raised by the reviewer.' },
      2: { response: { kind: 'partially_accepted', text: 'Added a mitigation; the owner is to be confirmed by Operations.' }, open: true, note: 'Still open: no owner is named.' },
      4: { response: { kind: 'accepted', text: 'Named the Director of Operations as owner, due 30 June 2027.' }, open: false, note: 'Resolved.' },
    },
  },
]

export function issueStep(def: IssueDef, round: number): IssueStep | null {
  if (round < def.raisedIn) return null
  let last: IssueStep | null = null
  for (let r = def.raisedIn; r <= round; r++) if (def.steps[r]) last = def.steps[r]
  return last
}

/** Author's summary of each revision (author_response_NN.json, shown in Review history). */
export const AUTHOR_SUMMARY: Record<number, { changes: string; accepted: number; rejected: number; partial: number; human: number }> = {
  1: { changes: 'Tightened the executive summary; no new claims.', accepted: 0, rejected: 0, partial: 0, human: 0 },
  2: { changes: 'Fixed the capital cost in the summary, sourced the forecast, added a risk mitigation.', accepted: 2, rejected: 1, partial: 2, human: 1 },
  3: { changes: 'Used the study\'s growth figure, added lease savings to Table 3, recalculated payback.', accepted: 2, rejected: 0, partial: 0, human: 0 },
  4: { changes: 'Named the owner of the labour risk mitigation.', accepted: 1, rejected: 0, partial: 0, human: 0 },
}
export const authorSummary = (round: number) => AUTHOR_SUMMARY[Math.min(round, 4)] ?? AUTHOR_SUMMARY[4]
