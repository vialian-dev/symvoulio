// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { matchesRun } from '../state/store'
// S9 Human decision: the run's numbers, the reviewer's highest status «Recommended for human
// review», and «This is not an approval.» until the human decides. Every decision asks for the decider's name and an
// optional note; Return starts another round on S7.
// A stopped run is not called completed: its title is «Review stopped» and the reviewer status
// «Stopped: human review required»; «Review completed» and «Recommended for human review» belong to completed runs only.
// «This is not an approval.» stays in both cases. After a decision the card shows who decided, when and the note, with
// Export one click away (Figma S09c).
import { ServiceError } from '../services/ServiceError'
import { useRunData } from '../services/useRunData'
import { ContributionWarning } from '../services/ContributionWarning'
import { HumanQuestions } from '../lib/HumanQuestionList'
import { useState } from 'react'
import { useNavigate, useParams } from 'react-router'
import { useTranslation } from 'react-i18next'
import { AnimatePresence, motion } from 'motion/react'
import { ArrowLeft, Check, FileDown, ListFilter, ShieldAlert, Undo2, X } from 'lucide-react'
import * as Tooltip from '@radix-ui/react-tooltip'
import { Button, Card, Caps, cx } from '@symvoulio/ui/components'
import type { ReactNode } from 'react'
import { fmt } from '@symvoulio/ui/i18n'
import { Page } from '../components/Page'
import { Dialog, DialogClose, DialogContent } from '../components/ui/dialog'
import { inputCls } from '../components/ui/controls'
import { openCounts } from '../lib/review'
import { useRunView } from '../services/useRunView'
import { finalHumanGate, canExport, canReturn as canReturnRun } from '../lib/runState'
import { decide, useStore, type Decision as D } from '../state/store'

type Kind = D['kind']

export function Decision() {
  const { t } = useTranslation()
  const nav = useNavigate()
  const { id } = useParams()
  const run = useStore((s) => s.runs.find((r) => matchesRun(r, id)))
  const [asking, setAsking] = useState<Kind | null>(null)
  const [name, setName] = useState('')
  const [note, setNote] = useState('')
  const view = useRunView(run)
  const { snapshot } = useRunData(id ?? '')
  const [pending, setPending] = useState(false)
  if (!run || !view) return <Page id="S9" caption={t('s9.caption')} title={t('s7.missing')}><Button onClick={() => nav('/reviews')}>{t('nav.reviews')}</Button></Page>

  const c = snapshot?.counts ?? openCounts(view.issues(run.done, run.issueDecisions))
  const passed = c.critical === 0 && c.high === 0
  const decided = run.state === 'approved' || run.state === 'rejected' ? run.decision : undefined
  const canReturn = canReturnRun(run)
  const stopped = view.stopped
  const humanGate = finalHumanGate(snapshot)
  // no verdict before any review.
  const reviewed = run.done > 0
  const confirm = async () => {
    if (!asking || !name.trim()) return
    const kind = asking
    if (pending) return
    setPending(true)
    const ok = await decide(run.id, kind, name.trim(), note.trim())
    setPending(false)
    if (ok === false) return
    setAsking(null); setNote('')
    if (kind === 'returned') nav(`/review/${run.id}`)
  }

  return (
    <Page id="S9" banner className="[overflow-wrap:anywhere]" caption={t('s9.caption')} title={t('s9.title')} accent={stopped ? t('s9.accentStopped') : t('s9.accent')}
      actions={<Button data-testid="back-to-workspace" onClick={() => nav(`/review/${run.id}`)}><ArrowLeft size={14} />{t('s9.backToWorkspace')}</Button>}>
      {!decided && (
        <Card data-testid="not-approval" className="mb-5 flex items-center gap-4 max-[600px]:flex-col max-[600px]:items-start border-[var(--value)] bg-[var(--value-soft)] px-6 py-5">
          <ShieldAlert size={28} className="shrink-0 text-[var(--value)]" />
          <div>
            <div className="serif-title text-[30px] leading-none font-bold">{t('s9.notApproval')}</div>
            <p className="mt-1.5 text-[13px] text-[var(--ink-2)]">{t('s9.notApprovalText')}</p>
          </div>
        </Card>
      )}
      <p className="-mt-2 mb-4 text-[13px] text-[var(--muted)]">{run.name} · {run.document.name}</p>
      <div className="grid grid-cols-[repeat(3,minmax(0,1fr))] gap-3 max-[1500px]:grid-cols-[repeat(2,minmax(0,1fr))] max-[600px]:grid-cols-1">
        <div data-testid="k-critical"><Kpi label={t('s9.critical')} value={c.critical} why={t('s9.why.critical')} tone={c.critical ? 'flag' : undefined} /></div>
        <div data-testid="k-high"><Kpi label={t('s9.high')} value={c.high} why={t('s9.why.high')} tone={c.high ? 'flag' : undefined} /></div>
        <div data-testid="k-medium"><Kpi label={t('s9.medium')} value={c.medium} why={t('s9.why.medium')} tone={c.medium ? 'value' : undefined} /></div>
        <div data-testid="k-threshold"><Kpi label={t('s9.threshold')} value={<span data-testid="threshold" className={!reviewed ? 'text-[var(--muted)]' : passed ? 'text-[var(--ok)]' : 'text-[var(--flag)]'}>{!reviewed ? t('s9.notReviewed') : passed ? t('s9.passed') : t('s9.failed')}</span>} why={reviewed ? t('s9.why.threshold') : t('s9.why.notReviewed')} /></div>
        <div data-testid="k-rounds"><Kpi label={t('s9.rounds')} value={`${run.done} / ${run.policy.maxRounds}`} why={t('s9.why.rounds')} /></div>
        <div data-testid="k-reviewer" className="min-w-0 [&_.num]:whitespace-normal"><Kpi label={t('s9.reviewerStatus')} value={<span data-testid="reviewer-status" style={{ whiteSpace: 'normal', overflowWrap: 'anywhere' }} className="text-[20px] leading-tight">{stopped ? t('s9.stoppedStatus') : humanGate ? t('s9.humanGate') : t('s9.recommended')}</span>} why={stopped ? t('s9.why.reviewerStopped') : t('s9.why.reviewer')} /></div>
      </div>

      {humanGate && <div data-testid="final-gate-reason" className="mt-3 text-[var(--ink-2)]">
        <p>{t('s9.humanGateReason')}</p>
        {snapshot?.rounds.at(-1)?.contributionExceeded && <p>{t('s9.humanGateContribution')}</p>}
        {!!snapshot?.humanQuestions.some(q => !q.resolved) && <p>{t('s9.humanGateQuestions')}</p>}
        {!!snapshot?.rounds.at(-1) && snapshot.rounds.at(-1)!.gate.counts.critical + snapshot.rounds.at(-1)!.gate.counts.high > 0 && <p>{t('s9.humanGateFindings')}</p>}
      </div>}
      <ServiceError id={run.id} />
      <ContributionWarning id={run.id} />
      <HumanQuestions run={run} />
      <AnimatePresence mode="wait">
        {!decided ? (
          <motion.div key="undecided" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}>
            <div className="mt-5 flex flex-wrap gap-2">
              <Button data-testid="review-unresolved" onClick={() => nav(`/review/${run.id}?filter=unresolved`)}><ListFilter size={14} />{t('s9.reviewUnresolved')}</Button>
              <Button data-testid="return" disabled={!canReturn} title={canReturn ? undefined : t('s9.returnDisabled')} onClick={() => setAsking('returned')}><Undo2 size={14} />{t('s9.return')}</Button>
              <span className="flex-1" />
              <Button data-testid="reject" variant="danger" onClick={() => setAsking('rejected')}><X size={14} />{t('s9.reject')}</Button>
              <Button data-testid="approve" variant="primary" onClick={() => setAsking('approved')}><Check size={14} />{t('s9.approve')}</Button>
            </div>
          </motion.div>
        ) : (
          <motion.div key="decided" initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }}>
            <Card data-testid="decided" className={cx('mt-5 flex items-center gap-5 px-6 py-5', decided.kind === 'approved' ? 'border-[var(--ok)] bg-[var(--ok-soft)]' : 'border-[var(--flag)] bg-[var(--flag-soft)]')}>
              <span aria-hidden className={cx('grid size-12 shrink-0 place-items-center rounded-full text-white', decided.kind === 'approved' ? 'bg-[var(--ok)]' : 'bg-[var(--flag)]')}>
                {decided.kind === 'approved' ? <Check size={24} strokeWidth={3} /> : <X size={24} strokeWidth={3} />}
              </span>
              <div className="min-w-0">
                <Caps>{t('s9.decision')}</Caps>
                <div data-testid="decided-state" className="serif-title mt-1 text-[30px] leading-none font-bold">{t(`decision.${decided.kind}`)}</div>
                <p data-testid="decided-by" className="mt-1.5 text-[13.5px] text-[var(--ink)]">{decided.name} · {fmt.dateTime(decided.at)} UTC</p>
                {decided.note ? <p data-testid="decided-note" className="mt-1 text-[13px] text-[var(--ink-2)] italic">{t('s9.noteShown', { note: decided.note })}</p> : null}
              </div>
            </Card>
            {canExport(run) && <div className="mt-5 flex justify-end">
              <Button data-testid="decided-export" variant="primary" onClick={() => nav(`/review/${run.id}/export`)}><FileDown size={14} />{stopped ? t('s7.exportPartial') : t('s7.export')}</Button>
            </div>}
          </motion.div>
        )}
      </AnimatePresence>

      <Dialog open={asking !== null} onOpenChange={(o) => { if (!o) setAsking(null) }}>
        {asking && (
          <DialogContent testId="confirm-dialog" title={t(`s9.confirm.${asking}`)} description={t('s9.confirmText')} closeLabel={t('common.close')}>
            <ServiceError id={run.id} />
            <form onSubmit={(e) => { e.preventDefault(); confirm() }} className="space-y-3">
              <label className="block text-[12.5px] text-[var(--muted)]">{t('s9.deciderName')}
                <input data-testid="decider-name" required autoFocus className={`${inputCls} mt-1`} value={name} onChange={(e) => setName(e.target.value)} placeholder={t('s9.deciderPh')} />
              </label>
              <label className="block text-[12.5px] text-[var(--muted)]">{t('s9.note')}
                <textarea data-testid="decider-note" rows={2} className={`${inputCls} mt-1`} value={note} onChange={(e) => setNote(e.target.value)} />
              </label>
              <div className="flex justify-end gap-2 pt-1">
                <DialogClose asChild><Button type="button">{t('common.cancel')}</Button></DialogClose>
                <Button data-testid="confirm" type="submit" variant={asking === 'rejected' ? 'danger' : 'primary'} disabled={pending || !name.trim()}>{t(`s9.confirmBtn.${asking}`)}</Button>
              </div>
            </form>
          </DialogContent>
        )}
      </Dialog>
    </Page>
  )
}

/**
 * A KPI card whose number explains itself (the family Kpi), with the explanation anchored at the value's start edge
 * (a centred 320 px tooltip over a short value at the left of the screen slid over the next card and the nav).
 * Focusable, so keyboard users get the same explanation.
 */
function Kpi({ label, value, why, tone }: { label: string; value: ReactNode; why: string; tone?: 'value' | 'flag' }) {
  return (
    <Card className="h-full min-w-0 px-5 py-4">
      <Caps>{label}</Caps>
      <div className={cx('num mt-1 text-[30px] leading-none font-semibold', tone === 'value' && 'text-[var(--value-ink)]', tone === 'flag' && 'text-[var(--flag)]')}>
        <Tooltip.Root delayDuration={120}>
          <Tooltip.Trigger asChild>
            <span tabIndex={0} data-explain="" style={{ whiteSpace: 'normal', overflowWrap: 'anywhere' }} className="cursor-help rounded-sm underline decoration-[var(--mist-400,#8aa0bd)] decoration-dotted underline-offset-4 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]">{value}</span>
          </Tooltip.Trigger>
          <Tooltip.Portal>
            <Tooltip.Content side="top" align="start" sideOffset={6} collisionPadding={12}
              className="z-50 max-w-xs rounded-lg border border-[var(--line)] bg-[var(--surface)] px-3 py-2 text-[12px] text-[var(--ink-2)] shadow-[var(--shadow-lift)]">
              {why}
              <Tooltip.Arrow className="fill-[var(--surface)]" />
            </Tooltip.Content>
          </Tooltip.Portal>
        </Tooltip.Root>
      </div>
    </Card>
  )
}
