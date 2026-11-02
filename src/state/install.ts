// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { useSyncExternalStore } from 'react'
import { getState, pasteCommand, checkAgain } from './store'
import { installPlan } from '../lib/installCommands'
import { installEnvironment } from '../services/backend'
import { isDesktop } from '../services/env'
import { i18n } from '../i18n'
import type { Provider } from '../data/sample'
type Progress = { hint?: boolean; message?: string; active?: boolean; locking?: boolean }
let progress: Partial<Record<Provider, Progress>> = {}
const listeners = new Set<() => void>()
function update(p: Provider, value: Progress) { progress = { ...progress, [p]: value }; listeners.forEach(f => f()) }
export function useInstall() { return useSyncExternalStore(f => { listeners.add(f); return () => { listeners.delete(f) } }, () => progress) }
export async function startInstall(p: Provider) {
  if (progress[p]?.locking) return
  const previous = progress[p]
  update(p, { ...previous, locking: true })
  try {
    if (!isDesktop && getState().shells[p].pending) throw new Error(i18n.t('s1.installPending'))
    const env = await installEnvironment()
    const shell = isDesktop ? (await import('../services/panelShells')).getPanelShell(p) ?? '' : getState().shellName[p]
    const plan = installPlan({ provider: p, os: env.os, shell, installed: getState().conn[p].installed })
    if (!plan.ok) throw new Error(plan.error + (plan.link ? `: ${plan.link}` : ''))
    await pasteCommand(p, plan.command, 'install')
    update(p, { hint: isDesktop, active: true })
    document.querySelector<HTMLTextAreaElement>(`[data-testid="term-${p}"] textarea`)?.focus()
  } catch (e) { update(p, { ...previous, locking: false, message: String(e instanceof Error ? e.message : e) }) }
}
/** Only a shell command-completion event calls this; shell lifetime and ordinary output never imply completion. */
export async function installExited(p: Provider, code: number | null, last: string) {
  if (!progress[p]?.active) return
  update(p, { active: false })
  await checkAgain()
  if (getState().conn[p].installed) return true
  if (code !== 0) update(p, { message: i18n.t('s1.installFailed', { provider: i18n.t(`provider.${p}`), code: code ?? i18n.t('s1.installUnavailable'), last }) })
  else if (!getState().conn[p].installed) update(p, { message: i18n.t('s1.installUndetected') })
  return getState().conn[p].installed
}

export function installPainted(testId: string) {
  const p = testId === 'term-claude' ? 'claude' : testId === 'term-codex' ? 'codex' : undefined
  if (p && progress[p]?.active && !progress[p]?.hint) update(p, { ...progress[p], hint: true })
}

export function installTimeout(p: Provider) {
  if (progress[p]?.active) update(p, { ...progress[p], message: 'Still running — check manually when the installer finishes' })
}
