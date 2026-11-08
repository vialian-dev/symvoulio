// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// S11 Reviews, the home screen: the list of runs with their state; Open, Restart from revision, Delete; «Open
// sample review» shows a finished demo deliberation without signing in; an empty list invites a new review.
// The hero banner of the product sits above the title (BANNER_PLACES: S11 is the home screen).
// At the minimum window (1280 px, panels at 38 %) every column and action fits without sideways scroll: a fixed
// table layout, one-line dates and roles, the name truncated with its full text in the tooltip.
import { useState } from 'react'
import { useNavigate } from 'react-router'
import { useTranslation } from 'react-i18next'
import { motion } from 'motion/react'
import { FilePlus2, FileText, History, PlayCircle, Sparkles, Trash2 } from 'lucide-react'
import { Badge, Button, Card, Caps } from '@symvoulio/ui/components'
import { fmt } from '@symvoulio/ui/i18n'
import { Page } from '../components/Page'
import { Dialog, DialogClose, DialogContent } from '../components/ui/dialog'
import { other } from '../data/sample'
import { deleteRun, openSample, useStore, type Run, type RunState } from '../state/store'
import { shortDate } from '../lib/runDate'

export const STATE_BADGE: Record<RunState, 'plain' | 'accent' | 'value' | 'flag' | 'ok'> = {
  failed: 'flag', running: 'ok', paused: 'value', stopped: 'flag', completed: 'accent', approved: 'ok', rejected: 'flag', returned: 'value',
}
export function StateBadge({ run }: { run: Run }) {
  const { t } = useTranslation()
  return <Badge tone={STATE_BADGE[run.state]} className="text-[11.5px]"><span data-testid="run-state">{t(`runState.${run.state}`)}</span></Badge>
}
// The run date is `shortDate` (lib/runDate): «Oct 30, 19:56» on every engine, the full date and zone in its tooltip (F8).
export const fileSize = (n: number) => (n >= 1_000_000 ? `${fmt.num(n / 1_000_000, 1)} MB` : n >= 1000 ? `${fmt.num(n / 1000, 0)} KB` : `${n} B`)

export function Reviews() {
  const { t } = useTranslation()
  const nav = useNavigate()
  const runs = useStore((s) => s.runs)
  const [restart, setRestart] = useState<Run | null>(null)
  const [rev, setRev] = useState(1)
  const [del, setDel] = useState<Run | null>(null)

  const openSampleReview = () => nav(`/review/${openSample()}`)

  return (
    <Page id="S11" banner caption={t('s11.caption')} title={t('s11.title')} accent={t('s11.accent')}
      actions={<>
        <Button data-testid="open-sample" onClick={openSampleReview}><Sparkles size={14} />{t('s11.openSample')}</Button>
        <Button data-testid="new-review" variant="primary" onClick={() => nav('/new')}><FilePlus2 size={14} />{t('nav.new')}</Button>
      </>}>
      {runs.length === 0 ? (
        <Card data-testid="s11-empty" className="flex flex-col items-center gap-3 px-8 py-14 text-center">
          <span className="grid size-14 place-items-center rounded-full bg-[var(--accent-soft)] text-[var(--accent)]"><FileText size={26} /></span>
          <h2 className="serif-title text-[26px] font-semibold">{t('s11.emptyTitle')}</h2>
          <p className="max-w-[460px] text-[13.5px] text-[var(--ink-2)]">{t('s11.emptyText')}</p>
          <div className="mt-2 flex gap-2">
            <Button data-testid="empty-new" variant="primary" onClick={() => nav('/new', { state: { sample: true } })}><FilePlus2 size={14} />{t('s11.emptyNew')}</Button>
            <Button onClick={openSampleReview}><Sparkles size={14} />{t('s11.openSample')}</Button>
          </div>
        </Card>
      ) : (
        <Card className="overflow-hidden">
          <table data-testid="runs" className="w-full table-fixed border-collapse text-[12.5px]">
            <colgroup>
              <col />
              <col className="w-[100px]" />
              <col className="w-[112px] min-[1700px]:w-[150px]" />
              <col className="w-[76px]" />
              <col className="w-[92px]" />
              <col className="w-[136px] min-[1700px]:w-[300px]" />
            </colgroup>
            <thead>
              <tr className="border-b border-[var(--line)] text-start">
                {['name', 'date', 'roles', 'rounds', 'state', 'actions'].map((h) => <th key={h} className="caps px-2 py-2.5 align-bottom text-start font-medium leading-snug">{t(`s11.col.${h}`)}</th>)}
              </tr>
            </thead>
            <tbody>
              {runs.map((r, i) => (
                <motion.tr key={r.id} data-testid={`run-${r.backendId ?? r.id}`} initial={{ opacity: 0 }} animate={{ opacity: 1 }} transition={{ delay: i * 0.03 }}
                  className="border-b border-[var(--line)]/60 hover:bg-[var(--surface-2)]/60">
                  <td className="px-2 py-2.5">
                    <button type="button" title={r.name} className="line-clamp-2 max-w-full text-start font-medium text-[var(--ink)] hover:text-[var(--accent)]" onClick={() => nav(`/review/${r.id}`)}>{r.name}</button>
                    <div className="mt-0.5 flex min-w-0 items-center gap-1.5">
                      {r.sample ? <Badge tone="accent" className="shrink-0 !px-1.5 !py-0 text-[10.5px]">{t('s11.sample')}</Badge> : null}
                      <span data-testid="run-doc" className="truncate text-[12px] text-[var(--muted)]" title={r.document.name}>{r.document.name}</span>
                    </div>
                  </td>
                  <td data-testid="run-date" className="truncate px-2 py-2.5 whitespace-nowrap text-[var(--ink-2)]" title={`${fmt.dateTime(r.createdAt)} UTC`}>{shortDate(r.createdAt)}</td>
                  <td data-testid="run-roles" className="px-2 py-2.5 leading-snug text-[var(--ink-2)]"><span className="whitespace-nowrap">{t(`provider.${r.author}`)} →</span> <span className="whitespace-nowrap">{t(`provider.${other(r.author)}`)}</span></td>
                  <td className="num px-2 py-2.5 text-[14px] whitespace-nowrap">{r.done} / {r.policy.maxRounds}</td>
                  <td className="px-2 py-2.5"><StateBadge run={r} /></td>
                  <td className="px-2 py-2.5">
                    <div className="flex gap-0.5 whitespace-nowrap [&_button]:!px-1.5">
                      <Button data-testid="open" variant="quiet" onClick={() => nav(`/review/${r.id}`)}><PlayCircle size={14} />{t('s11.open')}</Button>
                      <Button data-testid="restart" variant="quiet" title={t('s11.restart')} disabled={r.done === 0} onClick={() => { setRev(r.done); setRestart(r) }}><History size={14} /><span className="max-[1700px]:sr-only">{t('s11.restart')}</span></Button>
                      <Button data-testid="delete" variant="quiet" aria-label={t('s11.delete')} title={t('s11.delete')} onClick={() => setDel(r)}><Trash2 size={14} /></Button>
                    </div>
                  </td>
                </motion.tr>
              ))}
            </tbody>
          </table>
        </Card>
      )}

      <Dialog open={restart !== null} onOpenChange={(o) => { if (!o) setRestart(null) }}>
        {restart && (
          <DialogContent testId="restart-dialog" title={t('s11.restartTitle')} description={t('s11.restartText', { name: restart.name })} closeLabel={t('common.close')}>
            <Caps>{t('s11.pickRevision')}</Caps>
            <div className="mt-2 flex flex-wrap gap-2">
              {Array.from({ length: restart.done }, (_, i) => i + 1).map((n) => (
                <button key={n} type="button" data-testid={`rev-${n}`} aria-pressed={rev === n} onClick={() => setRev(n)}
                  className={`rounded-lg border px-3 py-1.5 text-[13px] ${rev === n ? 'border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]' : 'border-[var(--line)] text-[var(--ink-2)]'}`}>
                  {t('s7.revision', { n })}
                </button>
              ))}
            </div>
            <div className="mt-5 flex justify-end gap-2">
              <DialogClose asChild><Button>{t('common.cancel')}</Button></DialogClose>
              <Button data-testid="restart-go" variant="primary" onClick={() => {
                const r = restart
                setRestart(null)
                nav('/new', { state: { restart: { runId: r.id, revision: rev } } })
              }}>{t('s11.restartGo')}</Button>
            </div>
          </DialogContent>
        )}
      </Dialog>

      <Dialog open={del !== null} onOpenChange={(o) => { if (!o) setDel(null) }}>
        {del && (
          <DialogContent testId="delete-dialog" title={t('s11.deleteTitle')} description={t('s11.deleteText', { name: del.name })} closeLabel={t('common.close')}>
            <div className="flex justify-end gap-2">
              <DialogClose asChild><Button data-testid="delete-cancel">{t('common.cancel')}</Button></DialogClose>
              <Button data-testid="delete-confirm" variant="danger" onClick={() => { deleteRun(del.id); setDel(null) }}><Trash2 size={14} />{t('s11.delete')}</Button>
            </div>
          </DialogContent>
        )}
      </Dialog>
    </Page>
  )
}
