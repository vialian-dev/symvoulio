// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// F5: which help the left rail shows for a setup step and the provider state. Pure, so the check runs it in Node.
import { PROVIDERS, type Provider } from '../data/sample'
import type { Conn } from '../state/store'

export type HelpStep = 'system' | Provider
/** Which setup step the route is, or null where the rail shows no help. */
export function helpStep(pathname: string): HelpStep | null {
  const m = /^\/setup\/(system|claude|codex)\/?$/.exec(pathname)
  return m ? (m[1] as HelpStep) : null
}

/** The help message for a step and the provider state: an i18n key and its values. Pure, for the check. */
export function helpMessage(step: HelpStep, conn: Record<Provider, Conn>, installing: Partial<Record<Provider, boolean>>, signingIn: Record<Provider, boolean>, method: Partial<Record<Provider, 'subscription' | 'api_key'>> = {}): { key: string; provider?: Provider } {
  if (step === 'system') {
    const missing = PROVIDERS.find((p) => !conn[p].installed)
    if (missing) return { key: installing[missing] ? 'help.system.installing' : 'help.system.install', provider: missing }
    return { key: PROVIDERS.every((p) => conn[p].connected) ? 'help.system.done' : 'help.system.signIn' }
  }
  const c = conn[step]
  if (c.connected) return { key: 'help.connect.done', provider: step }
  if (!c.installed) return { key: 'help.connect.notInstalled', provider: step }
  if (method[step] === 'api_key') return { key: 'help.connect.apiKey', provider: step }
  return { key: signingIn[step] ? 'help.connect.waiting' : 'help.connect.signIn', provider: step }
}
