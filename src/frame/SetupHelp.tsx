// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// F5 (all install and sign-in help belongs at the bottom of the app's left panel, saying what to
// do): on the first three setup steps the bottom of the left rail says, in a few words, what to do now. The text follows
// the provider state (installed, installing, signing in, connected) and changes as it changes.
import { useLocation } from 'react-router'
import { useTranslation } from 'react-i18next'
import { LifeBuoy } from 'lucide-react'
import { PROVIDERS, type Provider } from '../data/sample'
import { helpMessage, helpStep } from '../lib/setupHelp'
import { useInstall } from '../state/install'
import { useStore } from '../state/store'
import { appSignIn, usePlatform } from '../services/platform'

export function SetupHelp() {
  const { t } = useTranslation()
  const { pathname } = useLocation()
  const conn = useStore((s) => s.conn)
  const signingIn = useStore((s) => s.signingIn)
  const signInMethod = useStore((s) => s.signInMethod)
  const install = useInstall()
  // macOS and Linux; Windows keeps the 0.3.0 rail (review r1).
  const step = appSignIn(usePlatform()) ? helpStep(pathname) : null
  if (!step) return null
  const installing = Object.fromEntries(PROVIDERS.map((p) => [p, !!install[p]?.active])) as Record<Provider, boolean>
  const { key, provider } = helpMessage(step, conn, installing, signingIn, signInMethod)
  const name = provider ? t(`provider.${provider}`) : ''
  return (
    <section data-testid="setup-help" data-step={step} data-help={key} aria-live="polite" aria-label={t('help.title')}
      className="mb-2 rounded-xl border border-[var(--accent)] bg-[var(--accent-soft)] p-2.5 text-start">
      <h2 className="flex items-center gap-1.5 text-[11.5px] font-semibold text-[var(--accent)]"><LifeBuoy size={13} aria-hidden />{t('help.title')}</h2>
      <p data-testid="setup-help-text" className="mt-1.5 text-[12px] leading-snug text-[var(--ink)]">{t(key, { name, account: provider ? t(`help.account.${provider}`) : '' })}</p>
    </section>
  )
}
