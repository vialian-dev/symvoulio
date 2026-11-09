// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// One store for the whole prototype: setup (connections, roles, policy, settings), the provider shells used by the
// wizard, and the review runs with their canned timers. Plain module + useSyncExternalStore: timers keep running while
// the user moves between screens. Setup is remembered in localStorage (first launch = no record); runs live in memory.
import { useSyncExternalStore } from 'react'
import { other, SAMPLE_CRITERIA, SAMPLE_DOC, SAMPLE_OBJECTIVE, SAMPLE_SUPPORTING, type Provider } from '../data/sample'
import { DEFAULT_POLICY, gateAfter, type HumanIssueDecision, type Policy } from '../lib/review'
import { STEPS_PER_ROUND, STEPS_PER_SIDE, type TermLine } from '../lib/script'
import { isDesktop } from '../services/env'
import { canReturn } from '../lib/runState'

export type RunState = 'failed' | 'running' | 'paused' | 'stopped' | 'completed' | 'approved' | 'rejected' | 'returned'
export interface FileChip { name: string; size: number; type: string }
export interface Decision { kind: 'approved' | 'rejected' | 'returned'; name: string; note: string; at: string; afterRound: number }
export interface Run {
  id: string
  backendId?: string
  startFailure?: string
  name: string
  createdAt: string
  author: Provider
  document: FileChip
  supporting: FileChip[]
  objective: string
  criteria: string[]
  policy: Policy
  /** Completed rounds. */
  done: number
  /** Steps done in the current round (0..STEPS_PER_ROUND-1). */
  step: number
  phase?: 'author' | 'reviewer' | null
  pendingRevision?: number | null
  /** Canned Return finishes exactly this round, matching the engine returned_until limit. */
  returnedUntil?: number
  state: RunState
  /** A provider failed (auth): the run waits, paused. */
  error?: { provider: Provider }
  stopReason?: 'user' | 'gate'
  decision?: Decision
  decisionLog: Decision[]
  issueDecisions: Record<string, HumanIssueDecision>
  sample?: boolean
}

export interface Conn { installed: boolean; version: string; connected: boolean; method: 'subscription' | 'api_key' | null }
export interface Shell { pasteError?: string; lines: TermLine[]; pending: string | null; pendingKind: 'login' | 'login_key' | 'status' | 'install' | null; loginRan: boolean }
export type ShellName = string

export interface State {
  setupDone: boolean
  author: Provider
  policy: Policy
  conn: Record<Provider, Conn>
  shells: Record<Provider, Shell>
  shellName: Record<Provider, ShellName>
  workspace: string
  runs: Run[]
  speed: number
  checking: boolean
  /** F2: «Sign in» started the provider's login in its panel and the app waits for the browser sign-in. */
  signingIn: Record<Provider, boolean>
  signInMethod: Record<Provider, 'subscription' | 'api_key'>
}

const KEY = 'symvoulio.setup'
const VERSIONS: Record<Provider, string> = { claude: '2.1.314', codex: '0.166.0' }
const freshConn = (): Record<Provider, Conn> => ({
  claude: { installed: true, version: VERSIONS.claude, connected: false, method: null },
  codex: { installed: true, version: VERSIONS.codex, connected: false, method: null },
})
const freshShell = (): Shell => ({ lines: [], pending: null, pendingKind: null, loginRan: false })

const ago = (h: number) => new Date(Date.parse('2026-10-30T18:00:00Z') - h * 3600_000).toISOString()

function seedRuns(): Run[] {
  const base = {
    supporting: SAMPLE_SUPPORTING, objective: SAMPLE_OBJECTIVE, criteria: SAMPLE_CRITERIA, policy: DEFAULT_POLICY,
    step: 0, decisionLog: [] as Decision[], issueDecisions: {},
  }
  const dec = (kind: Decision['kind'], name: string, note: string, at: string, afterRound: number): Decision => ({ kind, name, note, at, afterRound })
  const approved = dec('approved', 'Dana Whitfield', 'Medium issues accepted as known limits.', ago(20), 3)
  const rejected = dec('rejected', 'Dana Whitfield', 'Superseded by the board pack.', ago(70), 2)
  const returned = dec('returned', 'Sam Ortega', 'Name the owner of the labour risk before the lender sees it.', ago(2), 3)
  return [
    { ...base, id: 'sample', sample: true, name: 'Northwind hub expansion — business case', createdAt: ago(1.5), author: 'claude', document: SAMPLE_DOC, done: 3, state: 'completed', stopReason: 'gate' },
    { ...base, id: 'board-pack', name: 'Northwind hub — board pack draft', createdAt: ago(5), author: 'codex', document: { name: 'northwind_board_pack.docx', size: 48_230, type: 'docx' }, policy: { ...DEFAULT_POLICY, maxRounds: 2 }, done: 2, state: 'stopped', stopReason: 'gate' },
    { ...base, id: 'lender-summary', name: 'Northwind hub — lender summary', createdAt: ago(3), author: 'claude', document: { name: 'northwind_lender_summary.md', size: 4_102, type: 'md' }, done: 1, step: 1, state: 'paused', error: { provider: 'claude' } },
    { ...base, id: 'investment-memo', name: 'Northwind hub — investment memo', createdAt: ago(26), author: 'codex', document: { name: 'northwind_investment_memo.docx', size: 61_877, type: 'docx' }, done: 3, state: 'approved', stopReason: 'gate', decision: approved, decisionLog: [approved] },
    { ...base, id: 'operations-annex', name: 'Northwind hub — operations annex', createdAt: ago(4), author: 'claude', document: { name: 'northwind_operations_annex.pdf', size: 210_448, type: 'pdf' }, done: 3, state: 'returned', stopReason: 'gate', decision: returned, decisionLog: [returned] },
    { ...base, id: 'early-draft', name: 'Northwind hub — early draft', createdAt: ago(74), author: 'codex', document: { name: 'northwind_early_draft.txt', size: 9_310, type: 'txt' }, policy: { ...DEFAULT_POLICY, maxRounds: 2 }, done: 2, state: 'rejected', stopReason: 'gate', decision: rejected, decisionLog: [rejected] },
  ]
}

function load(): Pick<State, 'setupDone' | 'author' | 'policy' | 'conn' | 'shellName' | 'workspace'> | null {
  try {
    const s = localStorage.getItem(KEY)
    return s ? JSON.parse(s) : null
  } catch { return null }
}
function save(s: State) {
  try {
    localStorage.setItem(KEY, JSON.stringify({ setupDone: s.setupDone, author: s.author, policy: s.policy, conn: s.conn, shellName: s.shellName, workspace: s.workspace }))
  } catch { /* storage blocked: the prototype still works for this visit */ }
}

function initial(): State {
  const saved = load()
  const speedParam = Number(new URLSearchParams(location.search).get('speed'))
  return {
    setupDone: saved?.setupDone ?? false,
    author: saved?.author ?? 'claude',
    policy: saved?.policy ?? DEFAULT_POLICY,
    conn: saved?.conn ?? freshConn(),
    shells: { claude: freshShell(), codex: freshShell() },
    shellName: saved?.shellName ?? { claude: 'pwsh', codex: 'pwsh' },
    workspace: saved?.workspace ?? 'C:\\Users\\you\\Symvoulio',
    runs: isDesktop ? [] : seedRuns(),
    speed: speedParam > 0 && speedParam <= 50 ? speedParam : 1,
    checking: false,
    signingIn: { claude: false, codex: false },
    signInMethod: { claude: 'subscription', codex: 'subscription' },
  }
}

let state: State = initial()
const listeners = new Set<() => void>()
function set(patch: Partial<State> | ((s: State) => Partial<State>)) {
  const previous = state
  state = { ...state, ...(typeof patch === 'function' ? patch(state) : patch) }
  // Desktop run events must not overwrite setup preferences loaded by another page.
  if (!isDesktop || previous.setupDone !== state.setupDone) save(state)
  listeners.forEach((l) => l())
}
const subscribe = (l: () => void) => { listeners.add(l); return () => { listeners.delete(l) } }
/** Desktop mode (Tauri): the real backend behind the same store functions, loaded lazily so the browser bundle never
 *  runs it; the browser keeps the canned behaviour below. */
const desktop = (name: string, args: unknown[]) => import('../services/storeAdapter').then((m) => m.action(name, args))
if (isDesktop) void import('../services/storeAdapter').then((m) => m.connect(() => state, set))
export const getState = () => state
export function useStore<T>(sel: (s: State) => T): T {
  return useSyncExternalStore(subscribe, () => sel(state))
}

// ---------- setup ----------
function settingsChange(patch: Partial<State> | ((s: State) => Partial<State>)) {
  const previous = state
  set(patch)
  if (isDesktop) return desktop('settings', [previous])
}
/** Finishing the wizard leaves both panels at a clean prompt: no install transcript, no typed command (F4). */
export function finishSetup() {
  // macOS and Linux only; Windows keeps the 0.3.0 behaviour (review r1).
  void import('../services/platform').then(({ appSignIn, currentOs }) => {
    if (appSignIn(currentOs())) for (const p of ['claude', 'codex'] as const) void resetProviderPanel(p)
  })
  return settingsChange({ setupDone: true })
}
export function setAuthor(p: Provider) { return settingsChange({ author: p }) }
export function swapRoles() { return settingsChange(s => ({ author: other(s.author) })) }
export function setPolicy(p: Policy) { return settingsChange({ policy: p }) }
export function setShellName(p: Provider, n: ShellName) { return settingsChange(s => ({ shellName: { ...s.shellName, [p]: n } })) }
export function setWorkspace(w: string) { return settingsChange({ workspace: w }) }

export function checkAgain(): Promise<void> {
  if (isDesktop) return desktop('checkAgain', []).then(() => undefined)
  set({ checking: true })
  return new Promise((done) => setTimeout(() => { set({ checking: false }); done() }, 700 / state.speed))
}

function patchShell(p: Provider, f: (sh: Shell) => Shell) { set((s) => ({ shells: { ...s.shells, [p]: f(s.shells[p]) } })) }

/** «Paste command»: the command is typed at the prompt and NOT executed. The app never presses Enter. */
export function pasteCommand(p: Provider, command: string, kind: Shell['pendingKind']) {
  if (isDesktop) {
    patchShell(p, sh => ({ ...sh, pasteError: undefined }))
    return desktop('pasteCommand', [p, command, kind]).then(() => undefined).catch((error: unknown) => {
    if (kind === 'install') throw error
    patchShell(p, sh => ({ ...sh, pasteError: String(error instanceof Error ? error.message : error) }))
    })
  }

  patchShell(p, (sh) => ({ ...sh, pending: command, pendingKind: kind }))
}

/** The user pressed Enter in the panel: the stub CLI answers. */
export function pressEnter(p: Provider) {
  if (isDesktop) { void desktop('pressEnter', [p]); return }

  const sh = state.shells[p]
  const prompt = promptFor(p)
  if (!sh.pending) { patchShell(p, (x) => ({ ...x, lines: [...x.lines, { readable: prompt, raw: prompt, tone: 'cmd' }] })); return }
  const typed: TermLine = { readable: `${prompt}${sh.pending}`, raw: `${prompt}${sh.pending}`, tone: 'cmd' }
  const out: TermLine[] = []
  const say = (t: string, tone: TermLine['tone'] = 'out') => out.push({ readable: t, raw: t, tone })
  let loginRan = sh.loginRan
  let method: Conn['method'] = state.conn[p].method
  if (sh.pendingKind === 'login') {
    if (p === 'claude') { say('Opening browser to sign in…'); say('If the browser did not open, press c to copy the URL.'); say('Login successful.', 'done') }
    else { say('Starting local login server on http://localhost:1455.'); say('Opening browser for ChatGPT sign-in…'); say('Successfully logged in.', 'done') }
    loginRan = true; method = 'subscription'
  } else if (sh.pendingKind === 'login_key') {
    if (p === 'claude') { say('Opening browser to sign in with your Console account…'); say('Login successful.', 'done') }
    else { say('Reading API key from stdin…'); say('Successfully logged in.', 'done') }
    loginRan = true; method = 'api_key'
  } else if (sh.pendingKind === 'status') {
    loginRan = loginRan || state.conn[p].connected
    if (p === 'claude') say(loginRan ? `{ "loggedIn": true, "authMethod": "${method === 'api_key' ? 'api_key' : 'claude.ai'}" }` : '{ "loggedIn": false, "authMethod": "none" }', loginRan ? 'done' : 'error')
    else say(loginRan ? (method === 'api_key' ? 'Logged in using an API key' : 'Logged in using ChatGPT') : 'Not logged in', loginRan ? 'done' : 'error')
  }
  patchShell(p, (x) => ({ ...x, lines: [...x.lines, typed, ...out], pending: null, pendingKind: null, loginRan }))
  if (method !== state.conn[p].method) set((s) => ({ conn: { ...s.conn, [p]: { ...s.conn[p], method } } }))
}

/** «Check connection»: pastes the status command (not executed) and reads the stubbed status in the background. */
export function checkConnection(p: Provider): Promise<boolean> {
  if (isDesktop) return desktop('checkConnection', [p]).then(Boolean)
  pasteCommand(p, p === 'claude' ? 'claude auth status' : 'codex login status', 'status')
  return new Promise((done) => setTimeout(() => {
    const ok = state.shells[p].loginRan || state.conn[p].connected
    set((s) => ({ conn: { ...s.conn, [p]: { ...s.conn[p], connected: ok, method: ok ? (s.conn[p].method ?? 'subscription') : null } } }))
    done(ok)
  }, 600 / state.speed))
}

const LOGIN: Record<Provider, string> = { claude: 'claude auth login', codex: 'codex login' }

/** F2 «Sign in»: the app runs the provider's official login command in its panel (it types the command and presses
 *  Enter itself), so the user only finishes the sign-in in the browser. Resolves true once the provider reports
 *  connected; the panel is then reset to a clean prompt (F4). */
export async function signIn(p: Provider): Promise<boolean> {
  if (state.signingIn[p]) return false
  // Each attempt has its own number; «Cancel» or a newer attempt makes an older one stale, and a stale attempt never
  // connects, resets the panel or clears the newer attempt's state (review r1).
  const attempt = ++signInAttempts[p]
  const current = () => signInAttempts[p] === attempt
  set((s) => ({ signingIn: { ...s.signingIn, [p]: true } }))
  patchShell(p, (sh) => ({ ...sh, pasteError: undefined }))
  try {
    if (isDesktop) return Boolean(await desktop('signIn', [p, current]))
    patchShell(p, (sh) => ({ ...sh, pending: LOGIN[p], pendingKind: 'login' }))
    pressEnter(p)
    await new Promise((done) => setTimeout(done, 900 / state.speed))
    if (!current()) return false
    set((s) => ({ conn: { ...s.conn, [p]: { ...s.conn[p], connected: true, method: 'subscription' } } }))
    await resetProviderPanel(p)
    return true
  } catch (error) {
    if (current()) patchShell(p, (sh) => ({ ...sh, pasteError: String(error instanceof Error ? error.message : error) }))
    return false
  } finally {
    if (current()) set((s) => ({ signingIn: { ...s.signingIn, [p]: false } }))
  }
}
const signInAttempts: Record<Provider, number> = { claude: 0, codex: 0 }

/** «Cancel» while waiting for the browser sign-in: the app stops waiting (the attempt goes stale); the panel is left as
 *  it is. */
export function cancelSignIn(p: Provider) {
  signInAttempts[p]++
  set((s) => ({ signingIn: { ...s.signingIn, [p]: false } }))
}

/** The sign-in method chosen on S2/S3; the rail help follows it (F5, review r1). */
export function setSignInMethod(p: Provider, method: 'subscription' | 'api_key') {
  set((s) => ({ signInMethod: { ...s.signInMethod, [p]: method } }))
}

/** F4: a clean idle prompt in a provider panel. The desktop app restarts the panel's shell (a panel that is not on
 *  screen is closed and opens fresh next time); the browser stand clears its lines. */
export async function resetProviderPanel(p: Provider): Promise<void> {
  if (isDesktop) {
    const { resetPanel } = await import('../services/panelShells')
    if (!(await resetPanel(p).catch(() => false))) {
      const { closePanel } = await import('../services/backend')
      await closePanel(p).catch(() => { /* no panel yet */ })
    }
    return
  }
  patchShell(p, (sh) => ({ ...freshShell(), loginRan: sh.loginRan }))
}

export function promptFor(p: Provider) {
  const role = p === state.author ? 'author' : 'reviewer'
  return `PS ${state.workspace}\\workspaces\\${role}> `
}

// ---------- runs ----------
const timers = new Map<string, number>()
const STEP_MS = 850

function patchRun(id: string, f: (r: Run) => Partial<Run>) {
  set((s) => ({ runs: s.runs.map((r) => (r.id === id ? { ...r, ...f(r) } : r)) }))
}
export const matchesRun = (r: Run, id: string | undefined) => r.id === id || (id !== undefined && r.backendId === id)
export const runById = (id: string | undefined) => state.runs.find((r) => matchesRun(r, id))

function schedule(id: string) {
  clearTimeout(timers.get(id))
  timers.set(id, window.setTimeout(() => tick(id), STEP_MS / state.speed))
}
function tick(id: string) {
  const r = runById(id)
  if (!r || r.state !== 'running') return
  if (r.step + 1 < STEPS_PER_ROUND) { patchRun(id, () => ({ step: r.step + 1 })); schedule(id); return }
  const round = r.done + 1
  const gate = gateAfter(round, r.policy, r.issueDecisions)
  if (gate.kind === 'continue' && !(r.returnedUntil != null && round >= r.returnedUntil)) { patchRun(id, () => ({ done: round, step: 0 })); schedule(id); return }
  patchRun(id, () => ({ done: round, step: 0, returnedUntil: undefined, state: gate.kind === 'stopped' ? 'stopped' : 'completed', stopReason: 'gate' }))
}

export function startRun(input: Pick<Run, 'name' | 'author' | 'document' | 'supporting' | 'objective' | 'criteria' | 'policy'>): string {
  const id = `run-${Date.now().toString(36)}`
  const run: Run = { ...input, id, createdAt: new Date().toISOString(), done: 0, step: 0, state: 'running', decisionLog: [], issueDecisions: {} }
  set((s) => ({ runs: [run, ...s.runs] }))
  if (isDesktop) void desktop('startRun', [id])
  else schedule(id)
  return id
}
export function openSample(): string {
  if (!runById('sample')) set((s) => ({ runs: [seedRuns()[0], ...s.runs] }))
  return 'sample'
}
export function pauseRun(id: string) {
  if (id !== 'sample' && isDesktop) { void desktop('pauseRun', [id]); return }
  clearTimeout(timers.get(id)); patchRun(id, () => ({ state: 'paused' }))
}
export function resumeRun(id: string) {
  if (id !== 'sample' && isDesktop) { void desktop('resumeRun', [id]); return }

  const r = runById(id)
  if (!r || r.error) return
  patchRun(id, () => ({ state: 'running', decision: r.state === 'returned' ? undefined : r.decision }))
  schedule(id)
}
export function stopRun(id: string) {
  if (id !== 'sample' && isDesktop) { void desktop('stopRun', [id]); return }
  clearTimeout(timers.get(id)); patchRun(id, () => ({ state: 'stopped', stopReason: 'user', returnedUntil: undefined }))
}
/** An auth failure: the provider is no longer connected until the user signs in again. */
export function disconnect(p: Provider) {
  set((s) => ({ conn: { ...s.conn, [p]: { ...s.conn[p], connected: false } }, shells: { ...s.shells, [p]: { ...s.shells[p], loginRan: false } } }))
}
export function clearError(id: string) { patchRun(id, () => ({ error: undefined })) }
export function deleteRun(id: string) {
  if (id !== 'sample' && isDesktop) { void desktop('deleteRun', [id]); return }
  clearTimeout(timers.get(id)); set((s) => ({ runs: s.runs.filter((r) => r.id !== id) }))
}

export function decide(id: string, kind: Decision['kind'], name: string, note: string) {
  if (id !== 'sample' && isDesktop) { return desktop('decide', [id, kind, name, note]) }

  const r = runById(id)
  if (!r) return
  const d: Decision = { kind, name, note, at: new Date().toISOString(), afterRound: r.done }
  if (kind === 'returned') {
    // «Return for another round»: one more round is allowed and starts at once — never past the
    // maximum. A run stopped mid-round keeps its written revision: it continues that round from the
    // step it reached (the review) instead of replaying it.
    if (!canReturn(r)) return
    const midRound = r.state === 'stopped' && r.step > 0
    patchRun(id, () => ({ decision: d, decisionLog: [...r.decisionLog, d], state: 'running', step: midRound ? r.step : 0, returnedUntil: r.done + 1, policy: { ...r.policy, maxRounds: Math.max(r.policy.maxRounds, r.done + 1) } }))
    schedule(id)
    return
  }
  clearTimeout(timers.get(id))
  patchRun(id, () => ({ decision: d, decisionLog: [...r.decisionLog, d], state: kind, returnedUntil: undefined }))
}
export function decideIssue(id: string, issueId: string, choice: HumanIssueDecision['choice'], note: string, answeredBy?: string) {
  if (id !== 'sample' && isDesktop) { return desktop('decideIssue', [id, issueId, choice, note, answeredBy]) }

  patchRun(id, (r) => ({ issueDecisions: { ...r.issueDecisions, [issueId]: { choice, note, round: r.done } } }))
}

/** Which provider is working now (for the pulsing panel), or null. */
export function activeProvider(r: Run | undefined): Provider | null {
  if (!r || r.state !== 'running') return null
  if (r.phase !== undefined) return r.phase === null ? null : r.phase === 'author' ? r.author : other(r.author)
  return r.step < STEPS_PER_SIDE ? r.author : other(r.author)
}
/** The round shown in the status strip: the one in progress, or the last completed. */
// A run paused or stopped in the middle of a round keeps that round (Stop in round 4 must not show round 3).
export const shownRound = (r: Run) => (r.state === 'running' || ((r.state === 'paused' || r.state === 'stopped') && r.step > 0) ? r.done + 1 : Math.max(r.done, 1))
