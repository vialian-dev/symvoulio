// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// S13 About & disclaimer: subtitle, version, GitHub link (placeholder); the README statements with
// the product name Symvoulio; the Windows note on SmartScreen / Smart App Control with Microsoft's wording; known limitations; the copyright and licence.
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { backend } from '../services/backend'
import { isDesktop } from '../services/env'
import type { AppInfo } from '../services/types'
import { ExternalLink, MonitorCheck, ScrollText, TriangleAlert } from 'lucide-react'
import { Badge, Card, Caps } from '@symvoulio/ui/components'
import { Page } from '../components/Page'

const STATEMENTS = ['independence', 'account', 'human', 'trademark'] as const
const LIMITS = ['ocr', 'ppt', 'layout', 'formulas', 'macros', 'redlining', 'images'] as const

const OS_NAME: Record<string, string> = { windows: 'Windows', macos: 'macOS', linux: 'Linux' }

export function About() {
  const { t } = useTranslation()
  // Desktop: the running app's own version and platform (app_info); the browser stand keeps the prototype line.
  const [info, setInfo] = useState<AppInfo | null>(null)
  useEffect(() => { if (isDesktop) void backend.appInfo().then(setInfo, () => setInfo(null)) }, [])
  return (
    <Page id="S13" caption={t('s13.caption')} title="Symvoulio">
      <p data-testid="subtitle" className="-mt-3 mb-4 text-[16px] text-[var(--ink-2)]">{t('s13.subtitle')}</p>
      <div className="mb-6 flex flex-wrap items-center gap-3 text-[13px]">
        <Badge tone="accent" className="text-[12px]"><span data-testid="version">{info ? t('s13.versionDesktop', { version: info.version, os: OS_NAME[info.os] ?? info.os }) : t('s13.version')}</span></Badge>
        <span data-testid="copyright" className="text-[var(--ink-2)]">{t('s13.copyright')}</span>
        <span data-testid="github" className="inline-flex items-center gap-1 text-[var(--muted)]" title={t('s13.githubHint')}><ExternalLink size={13} />{t('s13.github')}</span>
      </div>

      <Card className="mb-4 p-5">
        <p data-testid="provider-disclosure" className="text-[13px] leading-relaxed text-[var(--ink-2)]">{t('providerDisclosure')}</p>
      </Card>
      <div className="grid gap-4 min-[1600px]:grid-cols-2">
        {STATEMENTS.map((k) => (
          <Card key={k} data-testid={`statement-${k}`} className="p-5">
            <div className="flex items-center gap-2"><ScrollText size={15} className="text-[var(--accent)]" /><h2 className="text-[14.5px] font-semibold">{t(`s13.st.${k}.title`)}</h2></div>
            <div className="mt-2 space-y-2 text-[13px] leading-relaxed text-[var(--ink-2)]">
              {(t(`s13.st.${k}.text`, { returnObjects: true }) as string[]).map((p, i) => <p key={i}>{p}</p>)}
            </div>
          </Card>
        ))}
      </div>

      <Card data-testid="windows-note" className="mt-4 p-5">
        <div className="flex items-center gap-2"><MonitorCheck size={15} className="text-[var(--accent)]" /><h2 className="text-[14.5px] font-semibold">{t('s13.win.title')}</h2></div>
        <p className="mt-2 text-[13px] leading-relaxed text-[var(--ink-2)]">{t('s13.win.why')}</p>
        <blockquote className="mt-3 border-s-2 border-[var(--accent)] ps-3 text-[13px] text-[var(--ink-2)] italic">{t('s13.win.smartscreen')}</blockquote>
        <p className="mt-2 text-[12.5px] text-[var(--muted)]">{t('s13.win.dialog')}</p>
        <blockquote className="mt-3 border-s-2 border-[var(--accent)] ps-3 text-[13px] text-[var(--ink-2)] italic">{t('s13.win.sac')}</blockquote>
        <p className="mt-2 text-[12.5px] text-[var(--muted)]">{t('s13.win.checksum')}</p>
      </Card>

      <Card data-testid="limitations" className="mt-4 p-5">
        <div className="flex items-center gap-2"><TriangleAlert size={15} className="text-[var(--value)]" /><Caps>{t('s13.limits')}</Caps></div>
        <ul className="mt-2 grid list-disc gap-x-8 gap-y-1 ps-5 text-[13px] text-[var(--ink-2)] min-[1500px]:grid-cols-2">
          {LIMITS.map((l) => <li key={l}>{t(`s13.limit.${l}`)}</li>)}
        </ul>
        <p className="mt-3 text-[12.5px] text-[var(--muted)]">{t('s13.limitsNote')}</p>
      </Card>

      <p data-testid="copyright-footer" className="mt-6 text-center text-[12px] text-[var(--muted)]">{t('s13.copyrightLine')}</p>
    </Page>
  )
}
