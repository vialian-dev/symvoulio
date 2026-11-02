// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// S4 Assign roles: the Author is chosen, the Independent Reviewer is the other provider. The same
// provider for both roles cannot be selected, and the screen says why. Panel headers follow at once.
import { useNavigate } from 'react-router'
import { useTranslation } from 'react-i18next'
import { motion } from 'motion/react'
import { ArrowLeft, ArrowLeftRight, ArrowRight, PenLine, ScanSearch, ShieldAlert } from 'lucide-react'
import { Button, Card, Caps } from '@symvoulio/ui/components'
import { Page, Stepper } from '../components/Page'
import { Segmented } from '../components/ui/controls'
import { other, PROVIDERS, type Provider } from '../data/sample'
import { setAuthor, swapRoles, useStore } from '../state/store'

export function Roles() {
  const { t } = useTranslation()
  const nav = useNavigate()
  const author = useStore((s) => s.author)
  const reviewer = other(author)

  const roleCard = (role: 'author' | 'reviewer', value: Provider, icon: React.ReactNode) => (
    <Card data-testid={`s4-${role}`} className="h-full p-5">
      <div className="flex items-center gap-2 text-[var(--accent)]">{icon}<Caps>{t(`role.${role}`)}</Caps></div>
      <p className="mt-1 mb-3 text-[12.5px] text-[var(--muted)]">{t(`s4.${role}Text`)}</p>
      <Segmented label={t(`role.${role}`)} value={value} testId={`s4-${role}-select`}
        onChange={(v) => setAuthor(role === 'author' ? v : other(v))}
        options={PROVIDERS.map((p) => ({
          value: p, label: t(`provider.${p}`), testId: `s4-${role}-${p}`,
          // The provider holding the other role cannot be picked here: it would grade its own work.
          disabled: role === 'reviewer' ? p === author : false,
          title: role === 'reviewer' && p === author ? t('s4.rule') : undefined,
        }))} />
      <motion.div key={value} initial={{ opacity: 0, y: 4 }} animate={{ opacity: 1, y: 0 }} className="serif-title mt-4 text-[26px] font-semibold" data-testid={`s4-${role}-name`}>
        {t(`provider.${value}`)}
      </motion.div>
    </Card>
  )

  return (
    <Page id="S4" caption={t('setup.caption')} title={t('s4.title')} accent={t('s4.accent')}>
      <Stepper current="roles" />
      <div className="grid grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-stretch gap-4 max-[1400px]:grid-cols-1">
        {roleCard('author', author, <PenLine size={16} />)}
        <Button data-testid="s4-swap" className="self-center justify-self-center" onClick={swapRoles}><ArrowLeftRight size={14} />{t('s4.swap')}</Button>
        {roleCard('reviewer', reviewer, <ScanSearch size={16} />)}
      </div>
      <Card data-testid="s4-rule" className="mt-5 flex items-start gap-3 border-[var(--accent)] px-5 py-4">
        <ShieldAlert size={20} className="mt-0.5 shrink-0 text-[var(--accent)]" />
        <div>
          <div className="serif-title text-[20px] font-semibold">{t('s4.rule')}</div>
          <p className="mt-1 text-[13px] text-[var(--ink-2)]">{t('s4.ruleText')}</p>
        </div>
      </Card>
      <div className="mt-6 flex items-center gap-2">
        <Button data-testid="back" onClick={() => nav('/setup/codex')}><ArrowLeft size={14} />{t('common.back')}</Button>
        <span className="flex-1" />
        <Button data-testid="next" variant="primary" onClick={() => nav('/setup/policy')}>{t('common.next')}<ArrowRight size={14} /></Button>
      </div>
    </Page>
  )
}
