// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// S12 Settings: the shell under each panel (the only place the shell is named, map 0.4), provider versions with
// Refresh, the policy defaults (→ S5), the workspace folder, «Run setup again» (→ S1), and the language: English only.
import { availableShells, installEnvironment } from '../services/backend'
import { shellHintKey } from '../lib/platformNote'
import { ServiceError } from '../services/ServiceError'
import { usePanelShell } from '../services/panelShells'
import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router'
import { useTranslation } from 'react-i18next'
import { FolderOpen, RefreshCw, RotateCcw, SlidersHorizontal } from 'lucide-react'
import { Badge, Button, Card, Caps } from '@symvoulio/ui/components'
import { Page } from '../components/Page'
import { inputCls } from '../components/ui/controls'
import { PROVIDERS, other } from '../data/sample'
import { checkAgain, setShellName, setWorkspace, useStore, type ShellName } from '../state/store'

function ActualShell({ panel }: { panel: 'claude' | 'codex' }) {
  const shell = usePanelShell(panel)
  return <span data-testid={`actual-shell-${panel}`}>Running shell: {shell ?? 'No running panel'}</span>
}

function Section({ title, children, testId }: { title: string; children: React.ReactNode; testId: string }) {
  return (
    <Card data-testid={testId} className="p-5">
      <Caps>{title}</Caps>
      <div className="mt-3">{children}</div>
    </Card>
  )
}

export function SettingsScreen() {
  const { t } = useTranslation()
  const nav = useNavigate()
  const s = useStore((x) => x)
  const [available, setAvailable] = useState<string[] | null>(null)
  useEffect(() => { void availableShells().then(setAvailable).catch(() => setAvailable([])) }, [])
  // F7: the shell note names the shells of this OS only (no PowerShell text on a Mac).
  const [os, setOs] = useState<string | undefined>()
  useEffect(() => { void installEnvironment().then(e => setOs(e.os)).catch(() => setOs(undefined)) }, [])
  const [saving, setSaving] = useState(false)
  const [ws, setWs] = useState(s.workspace)
  const p = s.policy

  return (
    <Page id="S12" caption={t('s12.caption')} title={t('s12.title')}>
      <ServiceError id="settings" />
      <div className="grid gap-4 min-[1600px]:grid-cols-2">
        <Section title={t('s12.shells')} testId="s12-shells">
          <div className="space-y-3">
            {PROVIDERS.map((pr) => (
              <label key={pr} className="flex flex-wrap items-center gap-3 text-[13px]">
                <span className="w-[200px] text-[var(--ink-2)]">{t(`provider.${pr}`)} · {t(`role.${pr === s.author ? 'author' : 'reviewer'}`)}</span>
                <ActualShell panel={pr} />
                {available === null ? <span role="status">{t('service.loading')}</span> : <select aria-label="Shell for next panel session" data-testid={`shell-${pr}`} value={s.shellName[pr]} disabled={saving} onChange={async (e) => { setSaving(true); try { await setShellName(pr, e.target.value as ShellName) } finally { setSaving(false) } }} className={`${inputCls} w-auto`}>
                  <>{available.map(shell => <option key={shell} value={shell}>{shell === 'pwsh' ? t('s12.pwsh') : shell === 'powershell' ? t('s12.winps') : shell}</option>)}</>
                </select>}
              </label>
            ))}
            <p data-testid="shell-hint" className="text-[12px] text-[var(--muted)]">{t(shellHintKey(os))}</p>
          </div>
        </Section>

        <Section title={t('s12.versions')} testId="s12-versions">
          <dl className="space-y-2 text-[13px]">
            {PROVIDERS.map((pr) => (
              <div key={pr} className="flex items-center gap-3">
                <dt className="w-[140px] text-[var(--ink-2)]">{t(`provider.${pr}`)}</dt>
                <dd className="num text-[15px]" data-testid={`ver-${pr}`}>{s.checking ? '…' : s.conn[pr].version}</dd>
                <dd>{s.conn[pr].connected ? <Badge tone="ok">{t('s1.connected')}</Badge> : <Badge tone="value">{t('s1.notConnected')}</Badge>}</dd>
              </div>
            ))}
          </dl>
          <Button data-testid="refresh-versions" className="mt-3" disabled={s.checking} onClick={() => void checkAgain()}><RefreshCw size={14} className={s.checking ? 'animate-spin' : undefined} />{t('s12.refresh')}</Button>
        </Section>

        <Section title={t('s12.policy')} testId="s12-policy">
          <ul className="space-y-1 text-[13px] text-[var(--ink-2)]">
            <li>{t('role.author')}: {t(`provider.${s.author}`)} · {t('role.reviewer')}: {t(`provider.${other(s.author)}`)}</li>
            <li>{t('s6.sum.rounds', { max: p.maxRounds, min: p.minRounds })}</li>
            <li>{t(`s5.contrib.${p.contribution}`)} · {t('s6.sum.newText', { pct: p.newContent === 'custom' ? p.customPct : p.newContent })}</li>
            <li>{t('s6.sum.human')}</li>
          </ul>
          <Button data-testid="edit-policy" className="mt-3" onClick={() => nav('/setup/policy?from=settings')}><SlidersHorizontal size={14} />{t('s12.editPolicy')}</Button>
        </Section>

        <Section title={t('s12.workspace')} testId="s12-workspace">
          <div className="flex gap-2">
            <input data-testid="workspace" aria-label={t('s12.workspace')} className={inputCls} value={ws} onChange={(e) => setWs(e.target.value)} onBlur={() => setWorkspace(ws.trim() || s.workspace)} />
            <Button data-testid="workspace-save" onClick={() => setWorkspace(ws.trim() || s.workspace)}><FolderOpen size={14} />{t('s12.saveFolder')}</Button>
          </div>
          <p className="mt-2 text-[12px] text-[var(--muted)]">{t('s12.workspaceHint')}</p>
        </Section>

        <Section title={t('s12.language')} testId="s12-language">
          <select disabled aria-label={t('s12.language')} className={`${inputCls} w-auto`}><option>English</option></select>
          <p className="mt-2 text-[12px] text-[var(--muted)]">{t('s12.languageHint')}</p>
        </Section>

        <Section title={t('s12.setup')} testId="s12-setup">
          <p className="text-[13px] text-[var(--ink-2)]">{t('s12.setupText')}</p>
          <Button data-testid="run-setup" className="mt-3" onClick={() => nav('/setup/system')}><RotateCcw size={14} />{t('s12.runSetup')}</Button>
        </Section>
      </div>
    </Page>
  )
}
