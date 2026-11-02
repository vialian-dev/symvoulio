// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// S5 Default review policy: rounds, reviewer contribution, maximum NEW TEXT (never
// «value»), stop conditions, and human approval — always, locked. The editor is reused by S6 «Policy» per review.
import { useEffect, useState } from 'react'
import { useNavigate, useSearchParams } from 'react-router'
import { useTranslation } from 'react-i18next'
import { ArrowLeft, Check, Lock } from 'lucide-react'
import { Button, Card, Caps } from '@symvoulio/ui/components'
import { Page, Stepper } from '../components/Page'
import { Checkbox, RadioCards, Segmented, inputCls } from '../components/ui/controls'
import type { Contribution, NewContent, Policy } from '../lib/review'
import { finishSetup, setPolicy, useStore } from '../state/store'

const ROUNDS = ['1', '2', '3', '4', '5']

function CustomPercentage({ value, onChange, prefix }: { value: number; onChange: (n: number) => void; prefix: string }) {
  const { t } = useTranslation()
  const [draft, setDraft] = useState(String(value))
  const [invalid, setInvalid] = useState(false)
  useEffect(() => { setDraft(String(value)) }, [value])
  const commit = () => {
    const n = Number(draft)
    if (!/^\d+$/.test(draft) || !Number.isInteger(n) || n < 0 || n > 100) { setInvalid(true); return }
    setInvalid(false); setDraft(String(n)); onChange(n)
  }
  return <div>
    <label className="flex items-center gap-2 text-[13px]">
      <input data-testid={`${prefix}-new-pct`} type="text" inputMode="numeric" value={draft} aria-label={t('s5.customPct')}
        aria-invalid={invalid} aria-describedby={invalid ? `${prefix}-pct-error` : undefined}
        onChange={e => setDraft(e.target.value)} onBlur={commit} className={`${inputCls} w-20`} />
      <span data-testid={`${prefix}-pct-unit`} className="whitespace-nowrap text-[var(--muted)]">{t('s5.pctNewText')}</span>
    </label>
    {invalid && <p id={`${prefix}-pct-error`} data-testid={`${prefix}-pct-error`} role="alert" className="mt-1 text-[12px] text-[var(--flag)]">{t('s5.pctInvalid')}</p>}
  </div>
}

export function PolicyEditor({ value, onChange, prefix = 'pol' }: { value: Policy; onChange: (p: Policy) => void; prefix?: string }) {
  const { t } = useTranslation()
  const set = (patch: Partial<Policy>) => onChange({ ...value, ...patch })
  return (
    <div className="space-y-5">
      <div className="grid grid-cols-2 gap-5 max-[1500px]:grid-cols-1">
        <div>
          <Caps>{t('s5.maxRounds')}</Caps>
          <div className="mt-2"><Segmented label={t('s5.maxRounds')} testId={`${prefix}-max`} value={String(value.maxRounds)}
            onChange={(v) => { const m = Number(v); set({ maxRounds: m, minRounds: Math.min(value.minRounds, m) }) }}
            options={ROUNDS.map((r) => ({ value: r, label: r, testId: `${prefix}-max-${r}` }))} /></div>
        </div>
        <div>
          <Caps>{t('s5.minRounds')}</Caps>
          <div className="mt-2"><Segmented label={t('s5.minRounds')} testId={`${prefix}-min`} value={String(value.minRounds)}
            onChange={(v) => set({ minRounds: Number(v) })}
            options={ROUNDS.map((r) => ({ value: r, label: r, disabled: Number(r) > value.maxRounds, testId: `${prefix}-min-${r}` }))} /></div>
          <p className="mt-1 text-[12px] text-[var(--muted)]">{t('s5.minHint')}</p>
        </div>
      </div>

      <div>
        <Caps>{t('s5.contribution')}</Caps>
        <div className="mt-2">
          <RadioCards<Contribution> label={t('s5.contribution')} value={value.contribution} onChange={(c) => set({ contribution: c })}
            options={(['verify', 'correct', 'improve'] as const).map((c) => ({ value: c, title: t(`s5.contrib.${c}`), text: t(`s5.contribText.${c}`), testId: `${prefix}-contrib-${c}` }))} />
        </div>
      </div>

      <div>
        <Caps>{t('s5.newText')}</Caps>
        <div className="mt-2 flex flex-wrap items-center gap-3">
          <Segmented<NewContent> label={t('s5.newText')} testId={`${prefix}-new`} value={value.newContent} onChange={(v) => set({ newContent: v })}
            options={[{ value: '0', label: '0 %' }, { value: '10', label: '10 %' }, { value: '25', label: '25 %' }, { value: 'custom', label: t('s5.custom'), testId: `${prefix}-new-custom` }]} />
          {value.newContent === 'custom' && (
            <CustomPercentage value={value.customPct} onChange={customPct => set({ customPct })} prefix={prefix} />
          )}
        </div>
        <p className="mt-1 text-[12px] text-[var(--muted)]">{t('s5.newTextHint')}</p>
      </div>

      <div>
        <Caps>{t('s5.stopWhen')}</Caps>
        <div className="mt-2 grid grid-cols-2 gap-x-6 max-[1500px]:grid-cols-1">
          {(['noCritical', 'noHigh', 'noNew', 'maxRounds'] as const).map((k) => (
            <Checkbox key={k} testId={`${prefix}-stop-${k}`} checked={value.stop[k]} label={t(`s5.stop.${k}`)} onChange={(v) => set({ stop: { ...value.stop, [k]: v } })} />
          ))}
        </div>
      </div>

      <div data-testid={`${prefix}-human`} className="flex items-start gap-3 rounded-xl border border-[var(--line)] bg-[var(--surface-2)] px-4 py-3">
        <Lock size={16} className="mt-0.5 shrink-0 text-[var(--accent)]" />
        <div>
          <div className="text-[13.5px]"><span className="font-semibold">{t('s5.human')}</span> <span className="ms-1 rounded-full bg-[var(--accent-soft)] px-2 py-0.5 text-[12px] font-medium text-[var(--accent)]">{t('s5.always')}</span></div>
          <p className="mt-1 text-[12.5px] text-[var(--muted)]">{t('s5.humanText')}</p>
        </div>
      </div>
    </div>
  )
}

export function PolicyScreen() {
  const { t } = useTranslation()
  const nav = useNavigate()
  const [params] = useSearchParams()
  const fromSettings = params.get('from') === 'settings'
  const policy = useStore((s) => s.policy)
  return (
    <Page id="S5" caption={fromSettings ? t('s12.caption') : t('setup.caption')} title={t('s5.title')} accent={t('s5.accent')}>
      {!fromSettings && <Stepper current="policy" />}
      <Card className="p-6"><PolicyEditor value={policy} onChange={setPolicy} /></Card>
      <div className="mt-6 flex items-center gap-2">
        <Button data-testid="back" onClick={() => nav(fromSettings ? '/settings' : '/setup/roles')}><ArrowLeft size={14} />{t('common.back')}</Button>
        <span className="flex-1" />
        {fromSettings
          ? <Button data-testid="save-defaults" variant="primary" onClick={() => nav('/settings')}><Check size={14} />{t('s5.save')}</Button>
          : <Button data-testid="finish" variant="primary" onClick={() => { finishSetup(); nav('/reviews') }}><Check size={14} />{t('s5.finish')}</Button>}
      </div>
    </Page>
  )
}
