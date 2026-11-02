// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// S1 System check: one card per provider CLI — installed, version, authentication — and the official install
// commands per OS. Continue works even when not connected, with a warning.
import { useState } from 'react'
import { useNavigate } from 'react-router'
import { useTranslation } from 'react-i18next'
import { AnimatePresence, motion } from 'motion/react'
import { AlertTriangle, CheckCircle2, CircleDashed, Copy, LifeBuoy, RefreshCw, XCircle } from 'lucide-react'
import { Badge, Button, Card, Caps } from '@symvoulio/ui/components'
import { Page, Stepper } from '../components/Page'
import { PROVIDERS, type Provider } from '../data/sample'
import { startInstall, useInstall } from '../state/install'
import { checkAgain, useStore } from '../state/store'

export const INSTALL: Record<Provider, { os: string; cmd: string }[]> = {
  claude: [
    { os: 'windows', cmd: 'irm https://claude.ai/install.ps1 | iex' },
    { os: 'windowsWinget', cmd: 'winget install Anthropic.ClaudeCode' },
    { os: 'macos', cmd: 'curl -fsSL https://claude.ai/install.sh | bash' },
  ],
  codex: [
    { os: 'windows', cmd: 'powershell -ExecutionPolicy ByPass -c "irm https://chatgpt.com/codex/install.ps1 | iex"' },
    { os: 'npm', cmd: 'npm install -g @openai/codex' },
    { os: 'macos', cmd: 'curl -fsSL https://chatgpt.com/codex/install.sh | sh' },
  ],
}

export function CopyLine({ cmd, testId }: { cmd: string; testId?: string }) {
  const { t } = useTranslation()
  const [copied, setCopied] = useState(false)
  return (
    <div className="flex items-center gap-2 rounded-lg border border-[var(--term-line)] bg-[var(--term-bg)] px-3 py-2">
      {/* the whole command is readable before it is copied: it wraps, never cut with «…». */}
      <code data-testid="cmd" className="min-w-0 flex-1 font-[family-name:var(--font-mono)] text-[12px] break-all text-[var(--ink-2)]">{cmd}</code>
      <button type="button" data-testid={testId} aria-label={t('common.copy')} onClick={() => { void navigator.clipboard?.writeText(cmd).catch(() => {}); setCopied(true); setTimeout(() => setCopied(false), 1200) }}
        className="flex shrink-0 items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-[var(--muted)] hover:text-[var(--accent)]">
        <Copy size={12} />{copied ? t('common.copied') : t('common.copy')}
      </button>
    </div>
  )
}

export function SystemCheck() {
  const { t } = useTranslation()
  const nav = useNavigate()
  const installs = useInstall()
  const conn = useStore((s) => s.conn)
  const checking = useStore((s) => s.checking)
  const [help, setHelp] = useState(false)
  const allConnected = PROVIDERS.every((p) => conn[p].connected)

  return (
    <Page id="S1" caption={t('setup.caption')} title={t('s1.title')} accent={t('s1.accent')}>
      <Stepper current="system" />
      <p className="mb-5 max-w-[720px] text-[14px] text-[var(--ink-2)]">{t('s1.lead')}</p>
      <p data-testid="provider-disclosure" className="mb-5 text-[14px] text-[var(--ink-2)]">{t('providerDisclosure')}</p>
      <div className="grid grid-cols-2 gap-4 max-[1500px]:grid-cols-1">
        {PROVIDERS.map((p) => {
          const c = conn[p]
          return (
            <Card key={p} data-testid={`s1-card-${p}`} className="p-5">
              <div className="flex items-center justify-between">
                <h2 className="serif-title text-[22px] font-semibold">{t(`provider.${p}`)}</h2>
                {checking ? <Badge><RefreshCw size={11} className="animate-spin" />{t('s1.checking')}</Badge> : null}
              </div>
              <dl className="mt-3 grid grid-cols-[140px_1fr] gap-y-2 text-[13.5px]">
                <dt className="text-[var(--muted)]">{t('s1.installed')}</dt>
                <dd data-testid={`s1-installed-${p}`} className="flex items-center gap-1.5">{c.installed ? <CheckCircle2 size={15} className="text-[var(--ok)]" /> : <XCircle size={15} className="text-[var(--flag)]" />}{c.installed ? t('common.yes') : t('common.no')}</dd>
                <dt className="text-[var(--muted)]">{t('s1.version')}</dt>
                <dd data-testid={`s1-version-${p}`} className="num text-[15px]">{checking ? '…' : c.version}</dd>
                <dt className="text-[var(--muted)]">{t('s1.auth')}</dt>
                <dd data-testid={`s1-auth-${p}`} className="flex items-center gap-1.5">
                  {c.connected ? <CheckCircle2 size={15} className="text-[var(--ok)]" /> : <CircleDashed size={15} className="text-[var(--value)]" />}
                  {c.connected ? t('s1.connected') : t('s1.notConnected')}
                </dd>
              </dl>
              {!c.installed && <Button className="mt-3" data-testid={`s1-install-${p}`} onClick={() => void startInstall(p)}>{t('s1.install')}</Button>}
              {!c.installed && installs[p]?.hint && <p role="status" data-testid={`s1-install-hint-${p}`}>{t('s1.installHint')}</p>}
              {!c.installed && installs[p]?.message && <p role="alert" data-testid={`s1-install-msg-${p}`}>{installs[p]?.message}</p>}
            </Card>
          )
        })}
      </div>

      <div className="mt-5 flex flex-wrap items-center gap-2">
        <Button data-testid="s1-check" onClick={() => void checkAgain()} disabled={checking}><RefreshCw size={14} />{t('s1.checkAgain')}</Button>
        <Button data-testid="s1-help" aria-expanded={help} onClick={() => setHelp((h) => !h)}><LifeBuoy size={14} />{t('s1.help')}</Button>
        <span className="flex-1" />
        <Button data-testid="s1-continue" variant="primary" onClick={() => nav('/setup/claude')}>{t('common.continue')}</Button>
      </div>
      {!allConnected && (
        <p data-testid="s1-warning" className="mt-3 flex items-center gap-2 text-[13px] text-[var(--value-ink)]"><AlertTriangle size={15} />{t('s1.warning')}</p>
      )}

      <AnimatePresence>
        {help && (
          <motion.div initial={{ opacity: 0, height: 0 }} animate={{ opacity: 1, height: 'auto' }} exit={{ opacity: 0, height: 0 }} className="overflow-hidden">
            <Card data-testid="s1-help-panel" className="mt-5 p-5">
              <Caps rules>{t('s1.helpTitle')}</Caps>
              <div className="mt-4 grid grid-cols-2 gap-6 max-[1500px]:grid-cols-1">
                {PROVIDERS.map((p) => (
                  <div key={p} className="space-y-2">
                    <h3 className="text-[14px] font-semibold">{t(`provider.${p}`)}</h3>
                    {INSTALL[p].map((i) => (
                      <div key={i.os}>
                        <div className="mb-1 text-[12px] text-[var(--muted)]">{t(`s1.os.${i.os}`)}</div>
                        <CopyLine cmd={i.cmd} testId={`s1-copy-${p}-${i.os}`} />
                      </div>
                    ))}
                    <p className="text-[12px] text-[var(--muted)]">{t(`s1.helpNote.${p}`)} <code data-testid={`check-cmd-${p}`} className="font-[family-name:var(--font-mono)] whitespace-nowrap text-[var(--ink-2)]">{p} --version</code></p>
                  </div>
                ))}
              </div>
            </Card>
          </motion.div>
        )}
      </AnimatePresence>
    </Page>
  )
}
