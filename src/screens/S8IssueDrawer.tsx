// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// S8 Issue detail, a drawer over S7: the structured finding, the author's response, the history of
// the issue across rounds, and «Decide» for an issue that needs a human decision. Opened by ?issue=<id>.
import { ServiceError } from '../services/ServiceError'
import { HumanQuestions } from '../lib/HumanQuestionList'
import { humanQuestions } from '../lib/humanQuestions'
import { useRunData } from '../services/useRunData'
import { useState } from 'react'
import { useSearchParams } from 'react-router'
import { useTranslation } from 'react-i18next'
import { Gavel } from 'lucide-react'
import { Badge, Button, Caps } from '@symvoulio/ui/components'
import { Dialog, SheetContent } from '../components/ui/dialog'
import { inputCls } from '../components/ui/controls'
import { SevLabel } from '../components/review'
import { other } from '../data/sample'
import { useRunView } from '../services/useRunView'
import { decideIssue, type Run } from '../state/store'

function Row({ k, children, testId }: { k: string; children: React.ReactNode; testId?: string }) {
  return (
    <div className="grid grid-cols-[150px_minmax(0,1fr)] gap-3 border-b border-[var(--line)]/60 py-2 text-[13px]">
      <dt className="font-[family-name:var(--font-mono)] text-[11.5px] text-[var(--muted)]">{k}</dt>
      <dd data-testid={testId} className="text-[var(--ink)]">{children}</dd>
    </div>
  )
}

export function IssueDrawer({ run }: { run: Run }) {
  const { t } = useTranslation()
  const [params, setParams] = useSearchParams()
  const iid = params.get('issue')
  const view = useRunView(run)!
  const { snapshot } = useRunData(run.id)
  const question = humanQuestions(snapshot).find(q => q.id === iid)
  const def = iid ? view.issueDef(iid) : undefined
  const snap = def ? view.issues(Math.max(run.done, 1), run.issueDecisions).find((s) => s.def.id === def.id) : undefined
  const [choice, setChoice] = useState<'accept' | 'keep'>('accept')
  const [note, setNote] = useState('')
  const [pending, setPending] = useState(false)
  const close = () => setParams((p) => { p.delete('issue'); return p })

  const history = def ? Array.from({ length: Math.max(run.done, 1) }, (_, i) => i + 1).filter((r) => r >= def.raisedIn && def.steps[r]) : []
  const author = t(`provider.${run.author}`), reviewer = t(`provider.${other(run.author)}`)

  return (
    <Dialog open={!!question || (!!def && !!snap)} onOpenChange={(o) => { if (!o) close() }}>
      {question && <SheetContent testId="question-drawer" title={question.id} closeLabel={t('common.close')}><HumanQuestions run={run} only={question.id} /></SheetContent>}
      {def && snap && !question && (
        <SheetContent testId="issue-drawer" title={<span className="flex items-center gap-3"><span className="font-[family-name:var(--font-mono)] text-[18px]">{def.id}</span><SevLabel s={def.severity} /></span>} closeLabel={t('common.close')}>
          <dl>
            <Row k="issue_id" testId="f-id">{def.id}</Row>
            <Row k="severity" testId="f-severity"><SevLabel s={def.severity} /></Row>
            <Row k="criterion" testId="f-criterion">{def.criterion}</Row>
            <Row k="finding" testId="f-finding">{def.finding}</Row>
            <Row k="evidence" testId="f-evidence">{def.evidence}</Row>
            <Row k="proposed_action" testId="f-action">{def.action}</Row>
            <Row k="human_decision_required" testId="f-hdr">{def.humanDecision ? 'true' : 'false'}</Row>
          </dl>

          <div className="mt-5">
            <Caps>{t('s8.authorResponse')}</Caps>
            {snap.step.response ? (
              <div data-testid="author-response" className="mt-2 rounded-xl border border-[var(--line)] bg-[var(--surface-2)] p-3">
                <Badge tone={snap.step.response.kind === 'accepted' ? 'ok' : snap.step.response.kind === 'human_decision_required' ? 'value' : 'plain'} className="font-[family-name:var(--font-mono)]">{snap.step.response.kind}</Badge>
                <p className="mt-2 text-[13px] text-[var(--ink-2)]">{snap.step.response.text}</p>
              </div>
            ) : <p className="mt-2 text-[13px] text-[var(--muted)]">{t('s8.noResponse', { name: author })}</p>}
          </div>

          <div className="mt-5">
            <Caps>{t('s8.history')}</Caps>
            <ol data-testid="issue-history" className="mt-2 space-y-2 border-s border-[var(--line)] ps-4">
              {history.map((r) => {
                const st = def.steps[r]
                return (
                  <li key={r} className="relative text-[12.5px]">
                    <span className="absolute -start-[21px] top-1 size-2.5 rounded-full border-2 border-[var(--surface)]" style={{ background: st.open ? 'var(--value)' : 'var(--ok)' }} />
                    <div className="font-medium text-[var(--ink)]">{t('s7.round', { n: r })}</div>
                    {st.response && <div className="text-[var(--ink-2)]">{author}: <span className="font-[family-name:var(--font-mono)] text-[11.5px]">{st.response.kind}</span> — {st.response.text}</div>}
                    <div className="text-[var(--muted)]">{reviewer}: {st.note}</div>
                  </li>
                )
              })}
            </ol>
          </div>

          {def.humanDecision && (
            <div className="mt-5 rounded-xl border border-[var(--value)]/60 bg-[var(--value-soft)] p-4" data-testid="decide-box">
              <ServiceError id={run.id} />
              <div className="flex items-center gap-2 text-[13.5px] font-semibold"><Gavel size={15} />{t('s8.decideTitle')}</div>
              {snap.decided ? (
                <p data-testid="decided" className="mt-2 text-[13px] text-[var(--ink-2)]">{t(`s8.decided.${snap.decided.choice}`)}{snap.decided.note ? ` — «${snap.decided.note}»` : ''}</p>
              ) : (
                <>
                  <div className="mt-3 flex flex-wrap gap-2" role="radiogroup" aria-label={t('s8.decideTitle')}>
                    {(['accept', 'keep'] as const).map((c) => (
                      <button key={c} type="button" role="radio" aria-checked={choice === c} data-testid={`decide-${c}`} onClick={() => setChoice(c)}
                        className={`rounded-lg border px-3 py-1.5 text-[13px] ${choice === c ? 'border-[var(--accent)] bg-[var(--surface)] text-[var(--accent)]' : 'border-[var(--line)] text-[var(--ink-2)]'}`}>{t(`s8.choice.${c}`)}</button>
                    ))}
                  </div>
                  <textarea data-testid="decide-note" rows={2} className={`${inputCls} mt-3`} placeholder={t('s8.notePh')} aria-label={t('s8.note')} value={note} onChange={(e) => setNote(e.target.value)} />
                  <Button data-testid="decide" variant="primary" className="mt-3" disabled={pending} onClick={async () => { if (pending) return; setPending(true); try { await decideIssue(run.id, def.id, choice, note.trim()) } finally { setPending(false) } }}><Gavel size={14} />{t('s8.decide')}</Button>
                </>
              )}
            </div>
          )}
        </SheetContent>
      )}
    </Dialog>
  )
}
