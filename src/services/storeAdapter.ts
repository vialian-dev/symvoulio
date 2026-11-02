// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { onRunSnapshot, onRunUpdated, desktopBackend as backend } from './backend'
import { loadAliases, persistAliases, pruneAliases } from './runAliases'
import { fileBytes, stagedToken, forgetStaged } from './files'
import { finalHumanGate } from '../lib/runState'
import { DEFAULT_POLICY } from '../lib/review'
import { toPolicy, viewPolicy } from './policy'
import type { Run, State, FileChip } from '../state/store'
import type { Provider, RunEvent, RunSnapshot, Settings, HumanDecision } from './types'
import { STEPS_PER_SIDE, type TermLine } from '../lib/script'
export const snapshots = new Map<string, RunSnapshot>()
export const runLines = new Map<string, Record<Provider, TermLine[]>>()
export const runProviders = new Map<string, Provider>()
export const serviceErrors = new Map<string, unknown>()
const ids = new Map<string, string>()
const newlyBound = new Set<string>()
let aliasWorkspace: string | undefined
let aliasesReady: Promise<void> = Promise.resolve()
function saveAliases() { if (aliasWorkspace) persistAliases(aliasWorkspace, ids) }
function useAliasWorkspace(workspace: string) {
  if (aliasWorkspace === workspace) return
  ids.clear(); newlyBound.clear()
  aliasWorkspace = workspace
  for (const [alias, id] of loadAliases(workspace)) ids.set(alias, id)
}
const aliasFor = (id: string) => [...ids].find(([, real]) => real === id)?.[0] ?? id
const failures = new Map<string, RunSnapshot>()
const barriers = new Map<string, number>()
const barrier = (alias: string) => barriers.get(alias) ?? 0
function blockPending(alias: string, snapshot = snapshots.get(alias)) {
  barriers.set(alias, barrier(alias) + 1)
  if (snapshot) failures.set(alias, snapshot)
}
const terminal = (state: Run['state']) => !['running', 'paused'].includes(state)
function storageFailure(alias: string, error: unknown) {
  blockPending(alias)
  const message = error instanceof Error ? error.message : String(error)
  serviceErrors.set(alias, new Error(`Storage error: could not read run state. ${message}`))
  update(s => ({ runs: s.runs.map(r => r.id === alias ? { ...r, state: r.state === 'running' ? 'paused' : r.state, error: undefined } : r) }))
}
export function refreshRun(id: string, alias = aliasFor(id)) {
  const epoch = barrier(alias)
  const before = snapshots.get(alias)
  return backend.getRun(id).then(s => { alias = aliasFor(alias); if (epoch === barrier(alias)) apply(s, alias); else { trace(alias, s, 'snapshot', 'received'); trace(alias, s, 'snapshot', 'ignored: read barrier changed') } }).catch(error => {
    alias = aliasFor(alias)
    // A failed read begun before a newer execution must not pause that execution.
    if (epoch === barrier(alias) && (before as (RunSnapshot & { generation?: number }) | undefined)?.generation === (snapshots.get(alias) as (RunSnapshot & { generation?: number }) | undefined)?.generation) storageFailure(alias, error)
    else {
      const message = error instanceof Error ? error.message : String(error)
      serviceErrors.set(alias, new Error(`Storage error: could not read run state. ${message}`))
      update(s => ({ ...s }))
    }
  })
}
let booted = false
let read: () => State
let update: (patch: Partial<State> | ((s: State) => Partial<State>)) => void
function chip(name: string): FileChip { return { name, size: 0, type: name.split('.').pop() ?? '' } }
function viewDecision(d: HumanDecision) { return { kind: d.kind, name: d.name, note: d.note, at: d.at, afterRound: d.after_round } }
function trace(alias: string, snapshot: RunSnapshot | undefined, kind: string, decision: string, state = snapshot?.state, backendId = snapshot?.id ?? ids.get(alias) ?? null) {
  if (typeof window === 'undefined') return
  const entries = (window as unknown as { __symvoulioTrace?: unknown[] }).__symvoulioTrace
  if (Array.isArray(entries)) entries.push({ time: new Date().toISOString(), alias, backendId, generation: snapshot?.generation ?? null, revision: snapshot?.revision ?? null, state: state ?? null, kind, decision })
}
function apply(s: RunSnapshot & { phase?: 'author' | 'reviewer' | null; pendingRevision?: number | null }, alias = aliasFor(s.id), _authoritative = false) {
  alias = aliasFor(alias)
  trace(alias, s, "snapshot", "received")
  const previous = snapshots.get(alias) ?? snapshots.get(s.id)
  // Contract v3 orders persisted snapshots, including explicit Return and Resume results.
  const pair = (value: RunSnapshot) => value as RunSnapshot & { generation?: number; revision?: number }
  const failure = failures.get(alias)
  if (failure && s.state === 'running') {
    const a = pair(s), b = pair(failure)
    if (a.generation != null && b.generation != null && (a.generation < b.generation || a.generation === b.generation)) { trace(alias, s, "snapshot", "ignored: failure barrier"); return }
  }
  // First binding retains the newer backend snapshot before duplicate-row cleanup.
  if (previous) {
    const a = pair(s), b = pair(previous)
    if (a.generation != null && b.generation != null) {
      if (a.generation < b.generation || (a.generation === b.generation && (a.revision ?? 0) < (b.revision ?? 0))) {
        trace(alias, s, "snapshot", `ignored: older snapshot; current=${b.generation}/${b.revision}; alias snapshot=${snapshots.has(alias)}`)
        if (snapshots.has(alias)) return
        s = previous
      }
    } else if (!_authoritative && (s.roundsDone < previous.roundsDone || s.decisionLog.length < previous.decisionLog.length || (terminal(previous.state) && !terminal(s.state)) || s.rounds.length < previous.rounds.length || s.documents.length < previous.documents.length)) {
      trace(alias, s, 'snapshot', 'ignored: legacy ordering')
      if (snapshots.has(alias)) return
      s = previous
    }
  }
  if (s.lastError?.kind === 'storage') blockPending(alias, s)
  if (s.lastError) serviceErrors.set(alias, s.lastError.kind === 'publication' ? { kind: 'publication', message: 'Decision saved; the package will be repaired' } : new Error(`${s.lastError.kind === 'storage' ? 'Storage error: ' : ''}${s.lastError.message}`))
  trace(alias, s, "snapshot", "applied")
  // Retain the full issue snapshots, including resolvedOnArrival, for runView and S7.
  snapshots.set(alias, s)
  snapshots.set(s.id, s)
  const run: Run = { id: alias, backendId: s.id, name: s.name, createdAt: s.createdAt, author: s.author, document: chip(s.primaryName), supporting: s.supportingNames.map(chip), objective: s.objective, criteria: s.criteria, policy: viewPolicy(s.policy), state: s.state === 'stopped' && finalHumanGate(s) ? 'completed' : s.state, done: s.roundsDone, step: s.phase === 'reviewer' || s.pendingRevision != null ? STEPS_PER_SIDE : 0, phase: s.phase, pendingRevision: s.pendingRevision, error: s.lastError?.kind === 'auth' ? { provider: s.lastError.provider ?? s.author } : undefined, stopReason: s.stopReason === 'user_stop' || s.stopReason === 'cancelled' ? 'user' : s.stopReason?.startsWith('gate_') ? 'gate' : undefined, decision: s.decision ? viewDecision(s.decision) : undefined, decisionLog: s.decisionLog.map(viewDecision), issueDecisions: Object.fromEntries(s.issues.flatMap(i => i.humanDecision ? [[i.issue.issue_id, i.humanDecision]] : [])) }
  update(st => ({ runs: st.runs.some(r => r.id === alias) ? st.runs.map(r => r.id === alias ? { ...r, ...run } : r) : [run, ...st.runs] }))
}
function events(alias: string, realId: () => string | undefined) {
  let refresh = false
  return (e: RunEvent) => {
    alias = aliasFor(alias)
    trace(alias, snapshots.get(alias), e.kind, "received event", e.kind === 'state' ? e.state : undefined)
    if (e.kind === 'line' || e.kind === 'phase') {
      runProviders.set(alias, e.provider)
      // A new object per event: the panels memoise on its identity.
      const lines = { ...(runLines.get(alias) ?? { claude: [], codex: [] }) }
      // Only controlled summaries are readable; provider text is an opt-in raw field.
      const text = e.kind === 'line' ? (e as typeof e & { summary?: string }).summary ?? e.kind : e.phase
      lines[e.provider] = [...lines[e.provider], { readable: text, raw: e.kind === 'line' ? e.raw ?? text : text, tone: e.kind === 'phase' ? 'event' as const : 'out' as const }].slice(-2000)
      runLines.set(alias, lines)
      if (realId()) runLines.set(realId()!, lines)
      if (e.kind === 'phase' && realId()) void refreshRun(realId()!, alias)
      update({})
    } else if (e.kind === 'providerError') {
      blockPending(alias)
      serviceErrors.set(alias, e.errorKind === 'storage' ? new Error(`Storage error: ${e.message}`) : e)
      update(s => ({ runs: s.runs.map(r => r.id === alias ? { ...r, state: terminal(r.state) ? r.state : 'paused', error: e.errorKind === 'auth' ? { provider: e.provider } : undefined } : r) }))
    } else {
      const id = realId()
      if (!id) { trace(alias, snapshots.get(alias), e.kind, 'deferred: backend id pending'); refresh = true; return }
      void refreshRun(id, alias)
    }
    trace(alias, snapshots.get(alias), e.kind, 'applied: event handled')
    if (refresh && realId()) { refresh = false; void refreshRun(realId()!, alias) }
  }
}
async function detect() {
  const found = await backend.detect()
  update(s => ({ conn: { ...s.conn, ...Object.fromEntries(found.map(d => [d.provider, { installed: d.installed, version: d.version ?? '', connected: d.connected === true, method: d.method === 'subscription' || d.method === 'api_key' ? d.method : null }])) } }))
}
export function connect(get: () => State, set: typeof update) {
  read = get; update = set
  if (booted) return
  booted = true
  void onRunUpdated(id => {
    const alias = aliasFor(id)
    trace(alias, snapshots.get(alias), 'run-updated', 'received event', undefined, id)
    void refreshRun(id)
    trace(alias, snapshots.get(alias), 'run-updated', 'applied: refresh requested', undefined, id)
  }).catch(() => {})
  void onRunSnapshot(s => { void aliasesReady.then(() => apply(s)) }).catch(() => {})
  aliasesReady = backend.getSettings().then(s => {
    useAliasWorkspace(s.workspace)
    update({ workspace: s.workspace, author: s.author, policy: viewPolicy(s.policy), shellName: { claude: s.shell.claude ?? '', codex: s.shell.codex ?? '' } })
    restoreFailedReviews(s.workspace)
  }).catch(e => { serviceErrors.set('settings', e); update({}) })
  void detect().catch(e => serviceErrors.set('detect', e))
  void backend.listRuns().then(async summaries => {
    await aliasesReady
    pruneAliases(ids, new Set(summaries.map(s => s.id)), newlyBound)
    saveAliases()
    update(st => ({ runs: [...st.runs, ...summaries.filter(s => !st.runs.some(r => r.id === aliasFor(s.id) || r.backendId === s.id)).map(s => ({ id: aliasFor(s.id), backendId: s.id, name: s.name, createdAt: s.createdAt, author: 'claude' as const, document: chip(''), supporting: [], objective: '', criteria: [], policy: DEFAULT_POLICY, done: s.roundsDone, step: 0, state: s.state, decisionLog: [], issueDecisions: {} }))] }))
    for (const summary of summaries) {
      try { apply(await backend.getRun(summary.id)); await backend.subscribeRun(summary.id, events(summary.id, () => summary.id)) }
      catch (e) { serviceErrors.set(aliasFor(summary.id), e); update({}) }
    }
  }).catch(e => serviceErrors.set('runs', e))
}
// Preflight failures have no backend run directory. Keep their review metadata in
// workspace-scoped WebView storage, alongside the existing persisted alias index.
const failedKey = (workspace: string) => `symvoulio.failedReviews.v1:${encodeURIComponent(workspace)}`
function saveFailedReviews(alias = 'runs') {
  try {
    const s = read()
    localStorage.setItem(failedKey(s.workspace), JSON.stringify(s.runs.filter(r => r.state === 'failed' && !r.backendId && !r.sample)))
  } catch (error) {
    // A history-write failure must not turn an already committed backend run into a failed review.
    serviceErrors.set(alias, new Error(`Could not save failed review history: ${String(error)}`))
    update({})
  }
}
function restoreFailedReviews(workspace: string) {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(failedKey(workspace)) ?? '[]')
    if (!Array.isArray(value)) return
    const runs = value.filter((r): r is Run => r && typeof r.id === 'string' && typeof r.name === 'string' && r.state === 'failed' && !r.backendId && r.document && r.policy && Array.isArray(r.criteria))
    for (const r of runs) if (r.startFailure) serviceErrors.set(r.id, new Error(r.startFailure))
    update(s => ({runs: [...s.runs, ...runs.filter(r => !s.runs.some(existing => existing.id === r.id))]}))
  } catch (error) { serviceErrors.set('runs', error) }
}
export async function action(name: string, args: unknown[]) {
  await aliasesReady
  const alias = typeof args[0] === 'string' ? aliasFor(args[0]) : ''
  const id = ids.get(alias) ?? alias
  trace(alias, snapshots.get(alias), name, 'action received')
  serviceErrors.delete(alias || name)
  update({})
  try {
    switch (name) {
      case 'settings': {
        const s = read()
        const settings: Settings = { workspace: s.workspace, shell: { claude: s.shellName.claude || null, codex: s.shellName.codex || null }, author: s.author, policy: toPolicy(s.policy) }
        const saved = await backend.setSettings(settings)
        useAliasWorkspace(saved.workspace); break
      }
      case 'checkAgain': update({ checking: true }); try { await detect() } finally { update({ checking: false }) }; break
      case 'pasteCommand': await backend.paste(alias as Provider, args[1] as string, args[2] === 'install'); break
      case 'pressEnter': await backend.ptyWrite(alias as Provider, '\r'); break
      case 'checkConnection':
        // Showing the status command is optional; a busy panel must not skip detection.
        try { await backend.paste(alias as Provider, alias === 'claude' ? 'claude auth status' : 'codex login status') } catch { /* Pending input stays untouched. */ }
        await detect(); return read().conn[alias as Provider].connected
      case 'startRun': {
        update(s => ({ runs: s.runs.map(r => r.id === alias ? { ...r, state: 'running', error: undefined, startFailure: undefined } : r) }))
        const run = read().runs.find(r => r.id === alias)!
        const tokens: string[] = [run.document, ...run.supporting].flatMap(file => { const token = stagedToken(file); return token ? [token] : [] })
        const stage = async (file: FileChip) => {
          const existing = stagedToken(file)
          if (existing) return existing
          const staged = await backend.stageFile(file.name, await fileBytes(file))
          tokens.push(staged.token)
          if (staged.error) throw new Error(staged.error)
          return staged.token
        }
        let committed = false
        try {
          const primaryToken = await stage(run.document)
          const supportingTokens: string[] = []
          for (const file of run.supporting) supportingTokens.push(await stage(file))
          let realId: string | undefined
          const s = await backend.startRun({ name: run.name, author: run.author, objective: run.objective, criteria: run.criteria, policy: toPolicy(run.policy), primaryToken, supportingTokens }, events(alias, () => realId))
          committed = true
          await aliasesReady; realId = s.id; ids.set(alias, s.id); newlyBound.add(s.id); saveAliases(); trace(alias, s, "alias", "backend id bound"); apply(s, alias)
          // An early global event may already have installed the backend-id entry.
          update(st => ({ runs: st.runs.filter(r => r.id !== s.id || r.id === alias) }))
          trace(alias, snapshots.get(alias) ?? snapshots.get(s.id), "alias", `backend duplicate removed; alias row=${read().runs.some(r => r.id === alias)}; alias snapshot=${snapshots.has(alias)}`)
          saveFailedReviews(alias)
          // Fetch once more to cover events delivered before run_start returned its id.
          try { apply(await backend.getRun(s.id), alias) } catch (error) { storageFailure(alias, error) }
        } finally {
          try { if (!committed && tokens.length) await backend.discard(tokens) } finally { [run.document, ...run.supporting].forEach(forgetStaged) }
        }
        break
      }
      case 'pauseRun': await backend.pause(id); await refreshRun(id, alias); break
      case 'resumeRun': if (read().runs.find(r => r.id === alias)?.state === 'failed') { await action('startRun', [alias]); break }; apply(await backend.resume(id), alias, true); break
      case 'stopRun': { const stopped = await backend.stop(id); if (stopped) apply(stopped, alias); else await refreshRun(id, alias); break }
      case 'deleteRun': if (read().runs.find(r => r.id === alias)?.backendId || snapshots.has(alias)) await backend.deleteRun(id); runLines.delete(id); snapshots.delete(id); update(s => ({ runs: s.runs.filter(r => r.id !== alias) })); snapshots.delete(alias); runLines.delete(alias); failures.delete(alias); barriers.delete(alias); ids.delete(alias); newlyBound.delete(id); saveAliases(); saveFailedReviews(); break
      case 'decide': {
        const result = await backend.decide(id, args[1] as 'approved' | 'rejected' | 'returned', args[2] as string, args[3] as string)
        apply(result, alias, true)
        if (result.publication === 'incomplete') serviceErrors.set(alias, { kind: 'publication', message: 'Decision saved; the package will be repaired' })
        update({})
        break
      }
      case 'decideIssue': apply(await backend.decideIssue(id, args[1] as string, args[2] as 'accept' | 'keep', args[3] as string, args[4] as string | undefined), alias); break
    }
    return true
  } catch (error) {
    serviceErrors.set(alias || name, error)
    if (name === 'settings' && args[0]) { const prev = args[0] as State; update({ author: prev.author, policy: prev.policy, shellName: prev.shellName, workspace: prev.workspace }) }
    if (name === 'startRun') update(s => ({ runs: s.runs.map(r => r.id === alias ? { ...r, state: 'failed', startFailure: String(error) } : r) }))
    if (name === 'startRun') saveFailedReviews(alias)
    update({})
    if (name === 'pasteCommand') throw error
    if (name === 'checkConnection') return false
    if (name === 'decide' || name === 'decideIssue') return false
  }
}
