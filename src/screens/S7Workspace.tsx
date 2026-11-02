// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { matchesRun } from '../state/store'
// S7 Review workspace, the main screen: document tabs Original | Current | Diff | Final, the Issues block with
// counters and list (S8 drawer on click), Review history by round with gate verdicts, and the bottom bar. The two
// provider panels at the right play the canned output while the run is Running. The banner place of S7 is a strip
// (BANNER_PLACES). Carried over from the Figma frames (FIGMA_TO_CODE.md): the side-by-side diff as a two-column table with
// change markers and the «Revision 1 → Revision 2» selector, the issues list as rows (id · severity pill · finding ·
// status), the counters beside the Issues title, and the round chips «✓ Author revision → ✓ Review → gate verdict».
import { ServiceError } from '../services/ServiceError'
import { ContributionWarning } from '../services/ContributionWarning'
import { HumanQuestions } from '../lib/HumanQuestionList'
import { useEffect, useMemo, useState } from 'react'
import { useNavigate, useParams, useSearchParams } from 'react-router'
import { useTranslation } from 'react-i18next'
import { AnimatePresence, motion } from 'motion/react'
import { AlertOctagon, ArrowRight, Check, ChevronDown, ChevronRight, CircleCheck, FileDown, Filter, Gavel, Info, Loader2, Pause, Play, Square, Undo2, X } from 'lucide-react'
import { Badge, Button, Card, Caps, cx } from '@symvoulio/ui/components'
import { fmt } from '@symvoulio/ui/i18n'
import { Page } from '../components/Page'
import { DiffInline, DiffSide, DocView, GateChip, SevDot, SEV_COLOR } from '../components/review'
import { Segmented, Tabs, TabsContent, TabsList, TabsTrigger } from '../components/ui/controls'
import { other, SAMPLE_DOC, SEVERITIES, type Severity } from '../data/sample'
import { runDiffWorker } from '../lib/diffWorker'
import type { BlockDiff } from '../lib/diff'
import { openCounts } from '../lib/review'
import { STEPS_PER_SIDE } from '../lib/script'
import { deleteRun, disconnect, pauseRun, resumeRun, stopRun, useStore } from '../state/store'
import { fileAvailable } from '../services/files'
import { StateBadge } from './S11Reviews'
import { canExport } from '../lib/runState'
import { IssueDrawer } from './S8IssueDrawer'
import { useRunView } from '../services/useRunView'
import { isDesktop } from '../services/env'

export function Workspace() {
  const { t } = useTranslation()
  const nav = useNavigate()
  const { id } = useParams()
  const [params, setParams] = useSearchParams()
  const run = useStore((s) => s.runs.find((r) => matchesRun(r, id)))
  const [tab, setTab] = useState('current')
  const [diffRev, setDiffRev] = useState<number | null>(null)
  const [diffView, setDiffView] = useState<'side' | 'inline'>('side')
  const [openRound, setOpenRound] = useState<number | null>(null)

  const done = run?.done ?? 0
  const view = useRunView(run)
  const issues = useMemo(() => (run && view ? view.issues(done, run.issueDecisions) : []), [run, view, done])
  const rev = Math.min(diffRev ?? done, done)
  const before = view?.doc(rev - 1), after = view?.doc(rev)
  const [diff, setDiff] = useState<BlockDiff[]>([])
  const [diffError, setDiffError] = useState('')
  useEffect(() => {
    setDiff([]); setDiffError('')
    if (tab !== 'diff' || rev <= 0 || !before || !after) return
    const worker = new Worker(new URL('../lib/diff.worker.ts', import.meta.url), { type: 'module' })
    return runDiffWorker(worker, { before, after }, setDiff, setDiffError)
  }, [tab, run?.id, rev, before, after])
  if (!run || !view) {
    return (
      <Page id="S7" caption={t('s7.caption')} title={t('s7.missing')}>
        <Button onClick={() => nav('/reviews')}>{t('nav.reviews')}</Button>
      </Page>
    )
  }

  const filter = params.get('filter') === 'unresolved'
  const counts = openCounts(issues)
  const shownIssues = filter ? issues.filter((i) => i.open) : issues
  const lastGate = done > 0 ? view.gate(done) : null
  const running = run.state === 'running'
  const canDecide = ['stopped', 'completed', 'approved', 'rejected'].includes(run.state)
  const finished = canDecide
  const inProgress = running || (run.state === 'paused' && run.step > 0) || run.state === 'returned'
  const currentRound = done + 1
  const phase = run.phase ?? (run.step < STEPS_PER_SIDE ? 'author' : 'reviewer')
  // The document follows the step: once the Author has finished revision N (reviewer phase), it is shown.
  const live = inProgress && run.state !== 'returned'
  // a run stopped in the middle of a round keeps what that round produced: once the Author finished revision N
  // it is shown (its review did not finish), as the panels show it.
  const cut = run.state === 'stopped' && run.step > 0
  const shownRev = run.pendingRevision ?? ((live || cut) && phase === 'reviewer' ? done + 1 : done)
  const liveName = t(`provider.${phase === 'author' ? run.author : other(run.author)}`)
  // Pause and Resume share the first place: «Resume» only while the run is paused.
  const resumable = run.state === 'paused' || run.state === 'returned'
  // Export belongs to a finished run: never while a round is running or half done.
  const exportable = canExport(run)

  const openIssue = (iid: string) => setParams((p) => { p.set('issue', iid); return p })
  const reconnect = () => {
    const p = run.error!.provider
    disconnect(p)
    nav(`/setup/${p}?return=${run.id}`)
  }

  return (
    <Page id="S7" banner caption={t('s7.caption')} title={run.name} className="!max-w-none pb-0">
      <div className="mb-4 -mt-2 flex flex-wrap items-center gap-2 text-[12.5px] text-[var(--muted)]">
        <StateBadge run={run} />
        <span>{t(`provider.${run.author}`)} → {t(`provider.${other(run.author)}`)}</span>
        <span>·</span><span data-testid="doc-name" className="truncate">{run.document.name}</span>
        {lastGate && <GateChip gate={run.stopReason === 'user' && run.state === 'stopped' ? { kind: 'stopped', key: 'stoppedHuman', counts } : lastGate} testId="gate-current" past={inProgress ? done : undefined} />}
      </div>

      <ServiceError id={run.id} />
      {view.documents.length === 0 && <p role="status">{t(run.state === 'failed' ? 'service.noDocument' : 'service.loading')}</p>}
      {run.state === 'failed' && <>
        {isDesktop && ![run.document, ...run.supporting].every(fileAvailable)
          ? <Button data-testid="recreate-review" onClick={() => nav('/new', { state: { recreate: run.id } })}>{t('service.recreate')}</Button>
          : <Button onClick={() => resumeRun(run.id)}>{t('service.retry')}</Button>}
        <Button onClick={() => { deleteRun(run.id); nav('/reviews') }}>{t('service.delete')}</Button>
      </>}
      <AnimatePresence>
        {run.error && (
          <motion.div initial={{ opacity: 0, y: -6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0 }}>
            <Card data-testid="provider-error" className="mb-4 flex flex-wrap items-center gap-3 border-[var(--flag)] px-5 py-3">
              <AlertOctagon size={20} className="text-[var(--flag)]" />
              <div className="min-w-0 flex-1">
                <div className="text-[14px] font-semibold">{t('s7.error.title', { name: t(`provider.${run.error.provider}`) })}</div>
                <div className="text-[12.5px] text-[var(--ink-2)]">{t('s7.error.text')}</div>
              </div>
              <Button data-testid="reconnect" variant="primary" onClick={reconnect}>{t('s7.error.reconnect')}</Button>
            </Card>
          </motion.div>
        )}
      </AnimatePresence>
      {run.decision && (
        <Card data-testid="decision-banner" className="mb-4 flex flex-wrap items-center gap-3 px-5 py-3">
          {run.decision.kind === 'returned' ? <Undo2 size={18} className="text-[var(--value)]" /> : <Gavel size={18} className="text-[var(--accent)]" />}
          <span className="text-[13.5px] font-semibold">{t(`decision.${run.decision.kind}`)}</span>
          <span className="text-[12.5px] text-[var(--muted)]">{t('s7.decidedBy', { name: run.decision.name, at: fmt.dateTime(run.decision.at) })}{run.decision.note ? ` — «${run.decision.note}»` : ''}</span>
        </Card>
      )}

      {/* The prototype note is true only in the browser's canned mode; the desktop app reads and sends the document. */}
      {!isDesktop && run.document.name !== SAMPLE_DOC.name && (
        <p data-testid="stub-note" className="mb-3 flex items-start gap-2 rounded-lg border border-dashed border-[var(--line)] px-3 py-2 text-[12.5px] text-[var(--muted)]">
          <Info size={14} className="mt-0.5 shrink-0 text-[var(--accent)]" />{t('s7.stubNote', { name: run.document.name })}
        </p>
      )}
      {!(run.state === 'failed' && view.documents.length === 0) && <Card className="p-0">
        <Tabs value={tab} onValueChange={setTab}>
          <div className="flex flex-wrap items-center gap-3 px-4 pt-2">
            <TabsList label={t('s7.docTabs')} className="border-0">
              <TabsTrigger value="original" testId="tab-original">{t('s7.tab.original')}</TabsTrigger>
              <TabsTrigger value="current" testId="tab-current">{t('s7.tab.current')}</TabsTrigger>
              <TabsTrigger value="diff" testId="tab-diff">{t('s7.tab.diff')}</TabsTrigger>
              <TabsTrigger value="final" testId="tab-final">{t('s7.tab.final')}</TabsTrigger>
            </TabsList>
            <span className="flex-1" />
            {tab === 'diff' && done > 0 && diff.length > 0 && (
              <div className="flex flex-wrap items-center gap-2">
                <label className="flex items-center gap-1.5 text-[12px] text-[var(--muted)]">{t('s7.revisionSel')}
                  <select data-testid="diff-rev" value={rev} onChange={(e) => setDiffRev(Number(e.target.value))}
                    className="rounded-md border border-[var(--line)] bg-[var(--surface-2)] px-2 py-1 text-[12.5px] text-[var(--ink)]">
                    {Array.from({ length: done }, (_, i) => i + 1).map((n) => <option key={n} value={n}>{t('s7.revVs', { n, prev: n === 1 ? t('s7.tab.original') : t('s7.revision', { n: n - 1 }) })}</option>)}
                  </select>
                </label>
                <Segmented size="sm" label={t('s7.diffView')} value={diffView} onChange={setDiffView} testId="diff-view"
                  options={[{ value: 'side', label: t('s7.sideBySide'), testId: 'diff-view-side' }, { value: 'inline', label: t('s7.inline'), testId: 'diff-view-inline' }]} />
              </div>
            )}
          </div>
          <div className="max-h-[46vh] min-h-[220px] overflow-y-auto border-t border-[var(--line)] px-6 py-5">
            <TabsContent value="original"><DocView blocks={view.doc(0)} testId="doc-original" /></TabsContent>
            <TabsContent value="current">
              {live
                ? <p data-testid="doc-note" className="mb-3 text-[12.5px] text-[var(--muted)]">{phase === 'author'
                  ? t(shownRev === 0 ? 's7.docNote.writingFirst' : 's7.docNote.writing', { name: liveName, n: done + 1, prev: shownRev })
                  : t('s7.docNote.reviewing', { name: liveName, n: shownRev })}</p>
                : cut
                  ? <p data-testid="doc-note" className="mb-3 text-[12.5px] text-[var(--muted)]">{phase === 'author'
                    ? t('s7.docNote.stoppedWriting', { name: liveName, n: done + 1, prev: shownRev })
                    : t('s7.docNote.stoppedReviewing', { n: shownRev })}</p>
                  : null}
              {shownRev > 0 && <Badge tone="accent" className="mb-3"><span data-testid="doc-current-rev">{t('s7.revision', { n: shownRev })}</span></Badge>}
              <DocView blocks={view.doc(shownRev)} testId="doc-current" />
            </TabsContent>
            <TabsContent value="diff">
              {diffError && <p role="alert">{diffError}</p>}
              {!diff.length && rev > 0 && !diffError && <p role="status">{t('service.loading')} <Button onClick={() => setTab('current')}>{t('common.cancel')}</Button></p>}
              {done === 0 ? <p className="text-[13px] text-[var(--muted)]">{t('s7.noDiffYet')}</p>
                : diffView === 'side' ? <DiffSide diff={diff} left={rev === 1 ? t('s7.tab.original') : t('s7.revision', { n: rev - 1 })} right={t('s7.revision', { n: rev })} />
                  : <DiffInline diff={diff} />}
            </TabsContent>
            <TabsContent value="final">
              {finished && done > 0
                ? <><Badge tone="ok" className="mb-3">{t('s7.finalCandidate', { n: done })}</Badge><DocView blocks={view.doc(done)} testId="doc-final" /></>
                : <p data-testid="final-pending" className="text-[13px] text-[var(--muted)]">{t('s7.finalPending')}</p>}
            </TabsContent>
          </div>
        </Tabs>
      </Card>}

      <div className="mt-4 grid gap-4 min-[1700px]:grid-cols-2">
        <Card className="p-5" data-testid="issues">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
            <div className="min-w-0">
              <h2 className="serif-title text-[22px] leading-none font-semibold">{t('s7.issues')}</h2>
              <span className="text-[12px] text-[var(--muted)]">{done > 0 ? t('s7.afterReview', { n: done }) : t('s7.noReviewYet')}</span>
            </div>
            <span className="flex-1" />
            <div className="grid grid-cols-4 gap-1.5">
              {SEVERITIES.map((s) => (
                <div key={s} data-testid={`count-${s}`} className="flex min-w-[78px] items-baseline gap-1.5 rounded-lg border border-[var(--line)] bg-[var(--surface-2)] px-2.5 py-1.5"
                  style={counts[s] ? { borderColor: `color-mix(in oklab, ${SEV_COLOR[s]} 45%, var(--line))` } : undefined}>
                  <motion.span key={counts[s]} initial={{ scale: 1.25, opacity: 0.4 }} animate={{ scale: 1, opacity: 1 }} className="num text-[22px] leading-none font-semibold" style={{ color: counts[s] ? SEV_COLOR[s] : undefined }}>
                    <span data-testid={`count-${s}-n`}>{counts[s]}</span>
                  </motion.span>
                  <span className="caps !text-[10px]">{t(`sev.${s}`)}</span>
                </div>
              ))}
            </div>
          </div>
          {filter && (
            <button type="button" data-testid="filter-chip" onClick={() => setParams((p) => { p.delete('filter'); return p })}
              className="mt-2 inline-flex items-center gap-1 rounded-full bg-[var(--accent-soft)] px-2.5 py-0.5 text-[12px] text-[var(--accent)]"><Filter size={11} />{t('s7.unresolvedOnly')}<X size={11} /></button>
          )}
          <ul className="mt-3 divide-y divide-[var(--line)]" data-testid="issue-list">
            {shownIssues.map((i) => (
              <li key={i.def.id}>
                <button type="button" data-testid={`issue-${i.def.id}`} onClick={() => openIssue(i.def.id)}
                  className="grid w-full grid-cols-[60px_auto_minmax(0,1fr)_auto_14px] items-center gap-3 rounded-lg px-2 py-2 text-start hover:bg-[var(--surface-2)]">
                  <span className="font-[family-name:var(--font-mono)] text-[11.5px] text-[var(--muted)]">{i.def.id}</span>
                  <SevPill s={i.def.severity} />
                  <span>
                    <span className={cx('text-[13px]', i.open ? 'text-[var(--ink)]' : 'text-[var(--muted)] line-through decoration-[var(--muted)]/50')}>{i.def.finding}</span>
                    {i.resolvedOnArrival != null && <span className="block text-[12px] text-[var(--muted)]">{t('s7.reportedAlreadyResolved', { n: i.resolvedOnArrival })}</span>}
                  </span>
                  <span data-testid="issue-status" className={cx('inline-flex items-center gap-1.5 text-[12px] whitespace-nowrap',
                    i.decided ? 'text-[var(--accent)]' : i.open ? (i.def.humanDecision ? 'text-[var(--value-ink)]' : 'text-[var(--ink-2)]') : 'text-[var(--ok)]')}>
                    <span aria-hidden className="size-1.5 rounded-full bg-current" />
                    {i.decided ? t('s7.issueDecided') : i.open ? (i.def.humanDecision ? t('s7.needsHuman') : t('s7.open')) : t('s7.resolved')}
                  </span>
                  <ChevronRight size={14} className="text-[var(--muted)]" />
                </button>
              </li>
            ))}
            {shownIssues.length === 0 && <li className="py-3 text-[13px] text-[var(--muted)]">{done === 0 ? t('s7.noReviewYet') : t('s7.noIssues')}</li>}
          </ul>
        </Card>

        <Card className="p-5" data-testid="history">
          <Caps>{t('s7.history')}</Caps>
          <ol className="mt-3 space-y-2">
            {Array.from({ length: done }, (_, i) => i + 1).map((n) => {
              const g = view.gate(n)
              if (!g) return null
              const s = view.authorSummary(n)
              const c = openCounts(view.issues(n, {}))
              const isOpen = openRound === n
              return (
                <li key={n} data-testid={`round-${n}`} className="rounded-xl border border-[var(--line)]">
                  <button type="button" data-testid={`round-${n}-toggle`} aria-expanded={isOpen} onClick={() => setOpenRound(isOpen ? null : n)} className="flex w-full flex-wrap items-center gap-2 px-3 py-2.5 text-start">
                    <span className="serif-title w-[78px] shrink-0 text-[16px] font-semibold">{t('s7.round', { n })}</span>
                    <span className="flex flex-wrap items-center gap-1.5 text-[12px] text-[var(--ink-2)]">
                      <StepChip done>{t('s7.authorRevision', { n })}</StepChip><ArrowRight size={12} className="text-[var(--muted)]" />
                      <StepChip done>{t('s7.reviewN', { n })}</StepChip><ArrowRight size={12} className="text-[var(--muted)]" />
                    </span>
                    <ContributionWarning id={run.id} round={n} />
                    <GateChip gate={g} testId={`gate-${n}`} />
                    <span className="flex-1" />
                    <ChevronDown size={14} className={cx('text-[var(--muted)] transition-transform', isOpen && 'rotate-180')} />
                  </button>
                  <AnimatePresence>
                    {isOpen && (
                      <motion.div initial={{ height: 0, opacity: 0 }} animate={{ height: 'auto', opacity: 1 }} exit={{ height: 0, opacity: 0 }} className="overflow-hidden">
                        <div data-testid={`round-${n}-detail`} className="space-y-2 border-t border-[var(--line)] px-3 py-3 text-[12.5px] text-[var(--ink-2)]">
                          <p><span className="font-medium text-[var(--ink)]">{t(`provider.${run.author}`)}:</span> {s.changes}</p>
                          <p data-testid={`round-${n}-responses`} className="text-[var(--muted)]">{t('s7.responses', { a: s.accepted, r: s.rejected, p: s.partial, h: s.human })}</p>
                          <p><span className="font-medium text-[var(--ink)]">{t(`provider.${other(run.author)}`)}:</span> {t('s7.reviewCounts', { c: c.critical, h: c.high, m: c.medium, l: c.low })}</p>
                          <Button variant="quiet" data-testid={`round-${n}-diff`} onClick={() => { setDiffRev(n); setTab('diff') }}>{t('s7.showDiff', { n })}</Button>
                        </div>
                      </motion.div>
                    )}
                  </AnimatePresence>
                </li>
              )
            })}
            {inProgress && (
              <li data-testid="round-live" className="flex flex-wrap items-center gap-1.5 rounded-xl border border-dashed border-[var(--accent)]/60 px-3 py-2.5 text-[12px]">
                <span className="serif-title w-[78px] shrink-0 text-[16px] font-semibold">{t('s7.round', { n: currentRound })}</span>
                {run.state === 'returned'
                  ? <span className="text-[12.5px] text-[var(--ink-2)]">{t('s7.waitingResume')}</span>
                  : <>
                    <StepChip done={phase === 'reviewer'} live={phase === 'author'} running={running}>{phase === 'author' ? t('s7.live.author', { name: liveName, n: currentRound }) : t('s7.authorRevision', { n: currentRound })}</StepChip>
                    <ArrowRight size={12} className="text-[var(--muted)]" />
                    <StepChip live={phase === 'reviewer'} running={running}>{phase === 'reviewer' ? t('s7.live.reviewer', { name: liveName, n: currentRound }) : t('s7.reviewN', { n: currentRound })}</StepChip>
                    <ArrowRight size={12} className="text-[var(--muted)]" />
                    <StepChip>{t('s7.gateVerdict')}</StepChip>
                  </>}
              </li>
            )}
            {done === 0 && !inProgress && <li className="text-[13px] text-[var(--muted)]">{t('s7.noRounds')}</li>}
          </ol>
        </Card>
      </div>

      <div data-testid="bottom-bar" className="sticky bottom-0 z-10 -mx-7 max-[1440px]:-mx-5 max-[1440px]:px-5 mt-5 flex flex-wrap items-center gap-2 border-t border-[var(--line)] bg-[color-mix(in_oklab,var(--bg)_88%,transparent)] px-7 py-3 backdrop-blur">
        {resumable
          ? <Button data-testid="resume" disabled={!!run.error} onClick={() => resumeRun(run.id)}><Play size={14} />{t('s7.resume')}</Button>
          : <Button data-testid="pause" disabled={!running} onClick={() => pauseRun(run.id)}><Pause size={14} />{t('s7.pause')}</Button>}
        <Button data-testid="stop" variant="danger" disabled={!(running || run.state === 'paused' || run.state === 'returned')} onClick={() => stopRun(run.id)}><Square size={13} />{t('s7.stop')}</Button>
        <span className="flex-1" />
        <Button data-testid="human-decision" variant="primary" disabled={!canDecide} title={canDecide ? undefined : t('s7.decisionDisabled')} onClick={() => nav(`/review/${run.id}/decision`)}><Gavel size={14} />{t('s7.humanDecision')}</Button>
        <Button data-testid="export" disabled={!exportable} title={exportable ? undefined : t('s7.exportDisabled')} onClick={() => nav(`/review/${run.id}/export`)}><FileDown size={14} />{view.stopped ? t('s7.exportPartial') : t('s7.export')}</Button>
        {run.state === 'completed' && <span className="flex items-center gap-1 text-[12px] text-[var(--ok)]"><CircleCheck size={13} />{t('s7.readyForHuman')}</span>}
      </div>

      <HumanQuestions run={run} />
      <IssueDrawer run={run} />
    </Page>
  )
}

/** A step of a round (Figma S07): done with a check, live with a pulse, or still to come (dashed). */
function StepChip({ children, done, live, running }: { children: React.ReactNode; done?: boolean; live?: boolean; running?: boolean }) {
  return (
    <span data-step={done ? 'done' : live ? 'live' : 'todo'} className={cx('inline-flex items-center gap-1 rounded-full border px-2 py-0.5 whitespace-nowrap',
      done && 'border-[var(--line)] bg-[var(--surface-2)] text-[var(--ink-2)]',
      live && 'border-[var(--ok)]/50 bg-[var(--ok-soft)] text-[var(--ok)]',
      !done && !live && 'border-dashed border-[var(--line)] text-[var(--muted)]')}>
      {done && <Check size={11} strokeWidth={3} className="text-[var(--ok)]" />}
      {live && (running ? <Loader2 size={11} className="animate-spin" /> : <Pause size={11} />)}
      {children}
    </span>
  )
}

/** Severity as a small pill (Figma issues list). */
function SevPill({ s }: { s: Severity }) {
  const { t } = useTranslation()
  return (
    <span className="inline-flex w-[76px] items-center gap-1.5 rounded-full border px-2 py-0.5 text-[11.5px] font-medium"
      style={{ color: SEV_COLOR[s], borderColor: `color-mix(in oklab, ${SEV_COLOR[s]} 40%, transparent)`, background: `color-mix(in oklab, ${SEV_COLOR[s]} 10%, transparent)` }}>
      <SevDot s={s} />{t(`sev.${s}`)}
    </span>
  )
}
