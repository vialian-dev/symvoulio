// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
export type Provider = 'claude' | 'codex'
export type Severity = 'critical' | 'high' | 'medium' | 'low'
export type ResponseKind = 'accepted' | 'rejected_with_evidence' | 'partially_accepted' | 'human_decision_required'
export type RunState = 'failed' | 'running' | 'paused' | 'stopped' | 'completed' | 'approved' | 'rejected' | 'returned'
export interface Counts { critical: number; high: number; medium: number; low: number }
export interface Policy { max_rounds: number; min_rounds: number; contribution: 'verify' | 'correct' | 'improve'; new_content_limit_pct: number; stop: { no_critical: boolean; no_high: boolean; no_new: boolean; max_rounds: boolean } }
export interface Issue { issue_id: string; severity: Severity; criterion: string; finding: string; evidence: string; proposed_action: string; human_decision_required: boolean; status: 'open' | 'resolved' }
export type StopReason = 'gate_no_new_issues' | 'gate_clean' | 'gate_max_rounds' | 'gate_human_review_required' | 'user_stop' | 'provider_error' | 'cancelled'
export interface Detection { provider: Provider; installed: boolean; version: string | null; connected: boolean | null; method: 'subscription' | 'api_key' | 'other' | null; status_exit_code: number | null; error: string | null }
export interface Usage { input_tokens: number; output_tokens: number; cache_read_tokens: number; cache_creation_tokens: number | null }
export interface BackendGate { kind: 'continue' | 'stop' | 'stopped'; reason: 'continue_open' | 'continue_min' | 'continue_new' | 'continue_contribution' | 'stop_no_new' | 'stop_clean' | 'stop_max' | 'stopped_human'; counts: Counts }
export interface RoundSnapshot { round: number; gate: BackendGate; newTextPct: number; contributionExceeded: boolean; authorUsage: Usage | null; reviewerUsage: Usage | null; authorModel: string | null; reviewerModel: string | null }
export interface IssueSnapshot { issue: Issue; raisedIn: number; resolvedOnArrival?: number; history: { round: number; status: 'open' | 'resolved'; severity: Severity; finding: string }[]; authorResponses: { round: number; response: ResponseKind; explanation: string; evidence: string }[]; humanDecision: { issue_id: string; choice: 'accept' | 'keep'; note: string; round: number } | null }
export type DecisionResult = RunSnapshot & { publication: 'complete' | 'incomplete' }
export interface RunSnapshot { generation: number; revision: number; id: string; name: string; createdAt: string; author: Provider; reviewer: Provider; policy: Policy; objective: string; criteria: string[]; state: RunState; stopReason: StopReason | null; lastError: { provider: Provider | null; kind: string; message: string } | null; phase: 'author' | 'reviewer' | null; pendingRevision: number | null; roundsDone: number; documents: string[]; rounds: RoundSnapshot[]; issues: IssueSnapshot[]; humanQuestions: { issue_id: string; question: string; round: number; resolved: boolean; answer: string | null; answeredAt: string | null; answeredBy: string | null }[]; counts: Counts; decision: HumanDecision | null; decisionLog: HumanDecision[]; outputTree: string[]; primaryName: string; supportingNames: string[] }
export interface RunSummary { id: string; name: string; state: RunState; roundsDone: number; createdAt: string; runDir: string }
export type RunEvent = { kind: 'line'; provider: Provider; role: 'author' | 'reviewer'; at: string; summary: string; raw?: string } | { kind: 'phase'; provider: Provider; role: 'author' | 'reviewer'; round: number; phase: string } | { kind: 'roundCompleted'; round: number; gate: BackendGate; newTextPct: number; contributionExceeded: boolean } | { kind: 'state'; state: RunState; stopReason: StopReason | null } | { kind: 'providerError'; provider: Provider; role: 'author' | 'reviewer'; errorKind: string; message: string }
export type PtyEvent = { kind: 'CommandTimeout' } | { kind: 'Data'; data: string } | { kind: 'Exit'; code: number } | { kind: 'CommandExit'; code: number | null; last: string }
export interface Shell { claude: string | null; codex: string | null }
export interface Settings { workspace: string; shell: Shell; author: Provider; policy: Policy }
export interface StagedFile { token: string; name: string; size: number; kind: string; chars: number; error: string | null }
export interface StartInput { name: string; objective: string; criteria: string[]; policy: Policy; author: Provider; primaryToken: string; supportingTokens: string[] }
export interface ExportResult { path: string; tree: string[] }
export interface AppInfo { version: string; os: string; arch: string }
export interface HumanDecision { kind: 'approved' | 'rejected' | 'returned'; name: string; note: string; at: string; after_round: number; revision: number; document_sha256: string }
export interface PtyOpenResult { shell: string; reattached: boolean }

export interface RestartInput { primaryToken: string; primaryName: string; supportingTokens: string[]; supportingNames: string[] }
