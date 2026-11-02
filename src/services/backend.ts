// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import type { DecisionResult, AppInfo, Detection, ExportResult, Provider, PtyEvent, PtyOpenResult, RunEvent, RunSnapshot, RunSummary, Settings, StagedFile, StartInput } from './types'
import { cannedBackend } from './canned'
export { isDesktop } from './env'
import { isDesktop } from './env'
export interface Backend {
  detect(): Promise<Detection[]>
  paste(panel: Provider, command: string, guard?: boolean, clipboard?: boolean): Promise<void>
  ptyOpen(panel: Provider, cwd: string | undefined, cols: number, rows: number, onEvent: (event: PtyEvent) => void, shell?: string): Promise<PtyOpenResult>
  ptyWrite(panel: Provider, data: string): Promise<void>
  ptyResize(panel: Provider, cols: number, rows: number): Promise<void>
  stageFile(name: string, bytes: number[]): Promise<StagedFile>
  startRun(input: StartInput, onEvent: (event: RunEvent) => void): Promise<RunSnapshot>
  subscribeRun(runId: string, onEvent: (event: RunEvent) => void): Promise<void>
  pause(runId: string): Promise<void>
  resume(runId: string): Promise<RunSnapshot>
  stop(runId: string): Promise<RunSnapshot>
  restart(runId: string, revision: number): Promise<{ primaryToken: string; primaryName: string; supportingTokens: string[]; supportingNames: string[] }>
  discard(tokens: string[]): Promise<void>
  decide(runId: string, kind: 'approved' | 'rejected' | 'returned', name: string, note: string): Promise<DecisionResult>
  decideIssue(runId: string, issueId: string, choice: 'accept' | 'keep', note: string, answeredBy?: string): Promise<RunSnapshot>
  getRun(runId: string): Promise<RunSnapshot>
  listRuns(): Promise<RunSummary[]>
  deleteRun(runId: string): Promise<void>
  exportRun(runId: string, partial: boolean): Promise<ExportResult>
  openFolder(path: string): Promise<void>
  appInfo(): Promise<AppInfo>
  getSettings(): Promise<Settings>
  setSettings(settings: Settings): Promise<Settings>
}
async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<T>(command, args)
}
async function channel<T>(onEvent: (event: T) => void) {
  const { Channel } = await import('@tauri-apps/api/core')
  const ch = new Channel<T>()
  ch.onmessage = onEvent
  return ch
}
export const desktopBackend: Backend = {
  detect: () => call('detect_providers'),
  paste: (panel, command, guard, clipboard) => call('pty_paste', { panel, command, guard, clipboard }),
  ptyOpen: async (panel, cwd, cols, rows, onEvent, shell) => call('pty_open', { panel, cwd, cols, rows, shell, onEvent: await channel(onEvent) }),
  ptyWrite: (panel, data) => call('pty_write', { panel, data }),
  ptyResize: (panel, cols, rows) => call('pty_resize', { panel, cols, rows }),
  stageFile: (name, bytes) => call('stage_file', { name, bytes }),
  startRun: async (input, onEvent) => call('run_start', { input, onEvent: await channel(onEvent) }),
  subscribeRun: async (runId, onEvent) => call('run_subscribe', { runId, onEvent: await channel(onEvent) }),
  pause: runId => call('run_pause', { runId }),
  resume: runId => call('run_resume', { runId }),
  stop: runId => call('run_stop', { runId }),
  restart: (runId, revision) => call('run_restart', { runId, revision }),
  discard: tokens => call('stage_discard', { tokens }),
  decide: (runId, kind, name, note) => call('run_decide', { runId, kind, name, note }),
  decideIssue: (runId, issueId, choice, note, answeredBy) => call('run_decide_issue', { runId, issueId, choice, note, answeredBy }),
  getRun: runId => call('run_get', { runId }),
  listRuns: () => call('runs_list'),
  deleteRun: runId => call('run_delete', { runId }),
  exportRun: (runId, partial) => call('run_export', { runId, partial }),
  openFolder: path => call('open_folder', { path }),
  appInfo: () => call('app_info'),
  getSettings: () => call('settings_get'),
  setSettings: settings => call('settings_set', { settings }),
}
export const backend = isDesktop ? desktopBackend : cannedBackend

export const closePanel = (panel: Provider) => call<void>('pty_close', { panel })

export async function onRunUpdated(callback: (id: string) => void) {
  const { listen } = await import('@tauri-apps/api/event')
  return listen<string>('run-updated', event => callback(event.payload))
}

export async function availableShells(): Promise<string[]> {
  if (!isDesktop) return ['pwsh', 'powershell']
  return (await call<{ available: string[] }>('shell_info')).available
}
export async function setRawOutput(enabled: boolean): Promise<void> {
  if (isDesktop) await call('raw_output_set', { enabled })
}

/** Authoritative committed state when a derived package cannot yet be published. */
export async function onRunSnapshot(callback: (snapshot: RunSnapshot) => void) {
  const { listen } = await import('@tauri-apps/api/event')
  return listen<RunSnapshot>('run-snapshot', event => callback(event.payload))
}

export async function installEnvironment(): Promise<{ os: 'windows' | 'macos' | 'linux' }> {
  return isDesktop ? call('shell_info') : { os: 'windows' }
}
