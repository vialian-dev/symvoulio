// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// S6 New review: name, objective, acceptance criteria approved by the human before the run, the
// primary document and supporting files with type checks, a per-review policy override, and a summary card.
import { ServiceError } from '../services/ServiceError'
import { useEffect, useRef, useState, type DragEvent, type ReactNode } from 'react'
import { useLocation, useNavigate } from 'react-router'
import { useTranslation } from 'react-i18next'
import { AnimatePresence, motion } from 'motion/react'
import { AlertCircle, CheckCircle2, ChevronDown, FileUp, ListPlus, Plus, Rocket, Sparkles, Upload, X } from 'lucide-react'
import { Badge, Button, Card, Caps, cx } from '@symvoulio/ui/components'
import { Page } from '../components/Page'
import { Field, inputCls } from '../components/ui/controls'
import { other, SAMPLE_CRITERIA, SAMPLE_DOC, SAMPLE_OBJECTIVE, SAMPLE_SUPPORTING, SUGGESTED_CRITERIA, TEMPLATES } from '../data/sample'
import { PolicyEditor } from './S5Policy'
import { fileSize } from './S11Reviews'
import { runById, startRun, useStore, type FileChip } from '../state/store'
import { isDesktop } from '../services/env'
import { desktopBackend } from '../services/backend'
import { snapshots } from '../services/storeAdapter'
import { registerFile, sampleFile, registerStaged, stagedToken } from '../services/files'
import type { Policy } from '../lib/review'

const PRIMARY = ['txt', 'md', 'docx', 'pdf']
const SUPPORTING = ['txt', 'md', 'docx', 'pdf', 'csv']
const ext = (n: string) => (n.includes('.') ? n.split('.').pop()!.toLowerCase() : '')

function Chip({ f, onRemove, testId }: { f: FileChip; onRemove: () => void; testId: string }) {
  const { t } = useTranslation()
  return (
    <motion.span layout initial={{ opacity: 0, scale: 0.96 }} animate={{ opacity: 1, scale: 1 }} data-testid={testId}
      className="inline-flex max-w-full items-center gap-2 rounded-lg border border-[var(--line)] bg-[var(--surface-2)] py-1 ps-2.5 pe-1 text-[12.5px]">
      <Badge tone="accent" className="uppercase">{f.type}</Badge>
      <span className="truncate text-[var(--ink)]" title={f.name}>{f.name}</span>
      <span className="shrink-0 text-[var(--muted)]">{fileSize(f.size)}</span>
      <button type="button" aria-label={t('s6.removeFile', { name: f.name })} onClick={onRemove} className="rounded p-0.5 text-[var(--muted)] hover:text-[var(--flag)]"><X size={13} /></button>
    </motion.span>
  )
}

function DropZone({ label, hint, accept, multiple, onFiles, testId, children }: { label: string; hint: string; accept: string[]; multiple?: boolean; onFiles: (f: File[]) => void; testId: string; children?: ReactNode }) {
  const { t } = useTranslation()
  const input = useRef<HTMLInputElement>(null)
  const [over, setOver] = useState(false)
  const drop = (e: DragEvent) => { e.preventDefault(); setOver(false); onFiles([...e.dataTransfer.files]) }
  return (
    <div data-testid={testId} onDragOver={(e) => { e.preventDefault(); setOver(true) }} onDragLeave={() => setOver(false)} onDrop={drop}
      className={cx('rounded-xl border border-dashed p-4 transition-colors', over ? 'border-[var(--accent)] bg-[var(--accent-soft)]' : 'border-[var(--line)] bg-[var(--surface-2)]/50')}>
      <div className="flex flex-wrap items-center gap-3">
        <span className="grid size-9 place-items-center rounded-full bg-[var(--surface)] text-[var(--accent)]"><Upload size={16} /></span>
        <div className="min-w-0 flex-1">
          <div className="text-[13.5px] font-medium">{label}</div>
          <div className="text-[12px] text-[var(--muted)]">{hint}</div>
        </div>
        <Button onClick={() => input.current?.click()}><FileUp size={14} />{t('s6.browse')}</Button>
        <input ref={input} data-testid={`${testId}-input`} type="file" className="hidden" multiple={multiple} accept={accept.map((a) => `.${a}`).join(',')}
          onChange={(e) => { onFiles([...(e.target.files ?? [])]); e.target.value = '' }} />
      </div>
      {children}
    </div>
  )
}

export function NewReview() {
  const { t } = useTranslation()
  const nav = useNavigate()
  const loc = useLocation() as { state: { sample?: boolean; recreate?: string; restart?: { runId: string; revision: number } } | null }
  const author = useStore((s) => s.author)
  const defaults = useStore((s) => s.policy)

  const [init] = useState(() => {
    const recreate = loc.state?.recreate ? runById(loc.state.recreate) : undefined
    if (recreate) return { name: recreate.name, objective: recreate.objective, criteria: recreate.criteria, policy: recreate.policy, doc: null, supporting: [] as FileChip[], restartFrom: 0 }
    const rs = loc.state?.restart
    const src = rs ? runById(rs.runId) : undefined
    if (src && rs) {
      return {
        name: t('s6.restartName', { name: src.name, n: rs.revision }), objective: src.objective, criteria: src.criteria, policy: src.policy,
        doc: isDesktop ? null : { name: `revision_${String(rs.revision).padStart(2, '0')}.md`, size: src.document.size, type: 'md' },
        supporting: isDesktop ? [] : src.supporting, restartFrom: rs.revision,
      }
    }
    if (loc.state?.sample) return { name: t('s6.sampleName'), objective: SAMPLE_OBJECTIVE, criteria: SAMPLE_CRITERIA, policy: defaults, doc: isDesktop ? null : SAMPLE_DOC, supporting: isDesktop ? [] : SAMPLE_SUPPORTING, restartFrom: 0 }
    return { name: '', objective: '', criteria: [] as string[], policy: defaults, doc: null as FileChip | null, supporting: [] as FileChip[], restartFrom: 0 }
  })

  const [name, setName] = useState(init.name)
  const [objective, setObjective] = useState(init.objective)
  const [criteria, setCriteria] = useState<string[]>(init.criteria)
  const [approved, setApproved] = useState(false)
  const [templates, setTemplates] = useState(false)
  const [doc, setDoc] = useState<FileChip | null>(init.doc)
  const [supporting, setSupporting] = useState<FileChip[]>(init.supporting)
  const [docError, setDocError] = useState<string | null>(null)
  const [supError, setSupError] = useState<string | null>(null)
  const [policyOpen, setPolicyOpen] = useState(false)
  const [policy, setPolicy] = useState<Policy>(init.policy)

  const restartRequest = useRef<ReturnType<typeof desktopBackend.restart> | null>(null)
  const started = useRef(false)
  const mounted = useRef(false)
  useEffect(() => {
    const rs = loc.state?.restart
    if (!isDesktop || !rs) return
    let active = true
    mounted.current = true
    restartRequest.current ??= desktopBackend.restart(snapshots.get(rs.runId)?.id ?? rs.runId, rs.revision)
    void restartRequest.current.then(result => {
      if (!active) return
      setDoc(registerStaged(result.primaryName, result.primaryToken))
      setSupporting(result.supportingNames.map((name, i) => registerStaged(name, result.supportingTokens[i])))
    }).catch(error => { if (active) setDocError(String(error)) })
    return () => {
      active = false
      mounted.current = false
      // A real unmount discards even tokens returned after navigation. StrictMode's replay shares the request.
      queueMicrotask(() => { if (!started.current) void restartRequest.current?.then(result => {
        if (!mounted.current && !started.current) return desktopBackend.discard([result.primaryToken, ...result.supportingTokens])
      }).catch(() => {}) })
    }
  }, [])
  const removeFile = (file: FileChip) => {
    const token = stagedToken(file)
    if (token) void desktopBackend.discard([token]).catch(error => setDocError(String(error)))
  }
  const loadSample = async () => {
    try { setDoc(isDesktop ? await sampleFile() : SAMPLE_DOC); setDocError(null) }
    catch (error) { setDocError(String(error)) }
  }
  useEffect(() => { if (isDesktop && loc.state?.sample) void loadSample() }, [])

  const editCriteria = (next: string[]) => { setCriteria(next); setApproved(false) }
  const cleanCriteria = criteria.map((c) => c.trim()).filter(Boolean)

  // The desktop app stages the file's bytes at start (src/services/files.ts); in the browser this is a no-op map entry.
  const toChip = (f: File): FileChip => registerFile({ name: f.name, size: f.size, type: ext(f.name) }, f)
  const onDoc = (files: File[]) => {
    const f = files[0]
    if (!f) return
    if (!PRIMARY.includes(ext(f.name))) { setDocError(t('s6.unsupported', { name: f.name, list: '.txt .md .docx .pdf' })); return }
    setDocError(null); if (doc) removeFile(doc); setDoc(toChip(f))
  }
  const onSupporting = (files: File[]) => {
    const bad = files.filter((f) => !SUPPORTING.includes(ext(f.name)))
    setSupError(bad.length ? t('s6.unsupported', { name: bad.map((b) => b.name).join(', '), list: '.txt .md .docx .pdf .csv' }) : null)
    setSupporting((s) => [...s, ...files.filter((f) => SUPPORTING.includes(ext(f.name))).map(toChip)])
  }

  const missing = [
    !name.trim() && t('s6.miss.name'),
    !objective.trim() && t('s6.miss.objective'),
    cleanCriteria.length === 0 && t('s6.miss.criteria'),
    !approved && t('s6.miss.approve'),
    !doc && t('s6.miss.document'),
  ].filter(Boolean) as string[]

  const start = () => {
    if (missing.length || !doc) return
    started.current = true
    const id = startRun({ name: name.trim(), author, document: doc, supporting, objective: objective.trim(), criteria: cleanCriteria, policy })
    nav(`/review/${id}`)
  }

  const suggest = () => {
    editCriteria([...criteria, ...SUGGESTED_CRITERIA.filter((s) => !criteria.includes(s))])
  }

  const policySummary = [
    t('s6.sum.rounds', { max: policy.maxRounds, min: policy.minRounds }),
    t(`s5.contrib.${policy.contribution}`),
    t('s6.sum.newText', { pct: policy.newContent === 'custom' ? policy.customPct : policy.newContent }),
    t('s6.sum.stops', { n: Object.values(policy.stop).filter(Boolean).length }),
    t('s6.sum.human'),
  ]

  return (
    <Page id="S6" caption={t('s6.caption')} title={t('s6.title')} accent={t('s6.accent')}>
      <ServiceError id="startRun" />
      <div className="grid gap-5 min-[1760px]:grid-cols-[minmax(0,1fr)_320px]">
        <div className="space-y-5">
          <Card className="space-y-4 p-5">
            {init.restartFrom > 0 && <span data-testid="restart-badge"><Badge tone="accent">{t('s6.restartBadge', { n: init.restartFrom })}</Badge></span>}
            <Field label={t('s6.name')} htmlFor="s6-name">
              <input id="s6-name" data-testid="s6-name" className={inputCls} value={name} onChange={(e) => setName(e.target.value)} placeholder={t('s6.namePh')} />
            </Field>
            <Field label={t('s6.objective')} htmlFor="s6-objective">
              <textarea id="s6-objective" data-testid="s6-objective" rows={2} className={inputCls} value={objective} onChange={(e) => setObjective(e.target.value)} placeholder={t('s6.objectivePh', { example: SAMPLE_OBJECTIVE })} />
            </Field>
          </Card>

          <Card className="p-5">
            <div className="flex flex-wrap items-center gap-2">
              <Caps>{t('s6.criteria')}</Caps>
              <span className="flex-1" />
              <Button data-testid="templates" aria-expanded={templates} onClick={() => setTemplates((v) => !v)}><ListPlus size={14} />{t('s6.fromTemplate')}<ChevronDown size={13} /></Button>
              <Button data-testid="suggest" onClick={suggest}><ListPlus size={14} />{t('s6.suggest')}</Button>
            </div>
            <p data-testid="suggest-note" className="mt-2 text-[12px] text-[var(--muted)]">{t('s6.suggestNote')}</p>
            <AnimatePresence>
              {templates && (
                <motion.div initial={{ opacity: 0, height: 0 }} animate={{ opacity: 1, height: 'auto' }} exit={{ opacity: 0, height: 0 }} className="overflow-hidden">
                  <div className="mt-3 grid grid-cols-3 gap-2 max-[1500px]:grid-cols-1">
                    {TEMPLATES.map((tp) => (
                      <button key={tp.id} type="button" data-testid={`template-${tp.id}`} onClick={() => { editCriteria(tp.criteria); setTemplates(false) }}
                        className="rounded-lg border border-[var(--line)] bg-[var(--surface-2)] p-3 text-start hover:border-[var(--accent)]">
                        <div className="text-[13px] font-medium">{t(`s6.template.${tp.id}`)}</div>
                        <div className="mt-0.5 text-[11.5px] text-[var(--muted)]">{t('s6.templateLines', { n: tp.criteria.length })}</div>
                      </button>
                    ))}
                  </div>
                </motion.div>
              )}
            </AnimatePresence>
            <ol className="mt-3 space-y-2" data-testid="criteria">
              {criteria.map((c, i) => (
                <li key={i} className="flex items-center gap-2">
                  <span className="num w-5 text-end text-[14px] text-[var(--muted)]">{i + 1}</span>
                  <input data-testid={`criterion-${i}`} aria-label={t('s6.criterionN', { n: i + 1 })} className={inputCls} value={c}
                    onChange={(e) => editCriteria(criteria.map((x, j) => (j === i ? e.target.value : x)))} />
                  {SUGGESTED_CRITERIA.includes(c) && <Badge tone="value">{t('s6.suggested')}</Badge>}
                  <button type="button" aria-label={t('s6.removeCriterion', { n: i + 1 })} onClick={() => editCriteria(criteria.filter((_, j) => j !== i))} className="rounded p-1 text-[var(--muted)] hover:text-[var(--flag)]"><X size={14} /></button>
                </li>
              ))}
            </ol>
            <div className="mt-3 flex flex-wrap items-center gap-3">
              <Button data-testid="add-criterion" variant="quiet" onClick={() => editCriteria([...criteria, ''])}><Plus size={14} />{t('s6.addLine')}</Button>
              <span className="flex-1" />
              {approved
                ? <span data-testid="criteria-approved" className="flex items-center gap-1.5 text-[13px] font-medium text-[var(--ok)]"><CheckCircle2 size={15} />{t('s6.approved')}</span>
                : <Button data-testid="approve-criteria" variant="primary" disabled={cleanCriteria.length === 0} onClick={() => { setCriteria(cleanCriteria); setApproved(true) }}><CheckCircle2 size={14} />{t('s6.approve')}</Button>}
            </div>
            <p className="mt-2 text-[12px] text-[var(--muted)]">{t('s6.approveHint')}</p>
          </Card>

          <Card className="space-y-4 p-5">
            <DropZone testId="primary" label={t('s6.primary')} hint={t('s6.primaryHint')} accept={PRIMARY} onFiles={onDoc}>
              <div className="mt-3 flex flex-wrap items-center gap-2">
                {doc ? <Chip f={doc} testId="doc-chip" onRemove={() => { removeFile(doc); setDoc(null) }} /> : (
                  <Button data-testid="use-sample" variant="quiet" onClick={() => void loadSample()}><Sparkles size={14} />{t('s6.useSample')}</Button>
                )}
              </div>
              {docError && <p data-testid="doc-error" role="alert" className="mt-2 flex items-center gap-1.5 text-[12.5px] text-[var(--flag)]"><AlertCircle size={14} />{docError}</p>}
            </DropZone>
            <DropZone testId="supporting" label={t('s6.supporting')} hint={t('s6.supportingHint')} accept={SUPPORTING} multiple onFiles={onSupporting}>
              {supporting.length > 0 && (
                <div className="mt-3 flex flex-wrap gap-2">
                  {supporting.map((f, i) => <Chip key={`${f.name}-${i}`} f={f} testId="sup-chip" onRemove={() => { removeFile(f); setSupporting((s) => s.filter((_, j) => j !== i)) }} />)}
                </div>
              )}
              {supError && <p data-testid="sup-error" role="alert" className="mt-2 flex items-center gap-1.5 text-[12.5px] text-[var(--flag)]"><AlertCircle size={14} />{supError}</p>}
            </DropZone>
          </Card>

          <Card className="p-5">
            <button type="button" data-testid="policy-toggle" aria-expanded={policyOpen} onClick={() => setPolicyOpen((v) => !v)} className="flex w-full items-center gap-2 text-start">
              <Caps>{t('s6.policy')}</Caps>
              <span className="text-[12px] text-[var(--muted)]">{t('s6.policyHint')}</span>
              <span className="flex-1" />
              <ChevronDown size={16} className={cx('text-[var(--muted)] transition-transform', policyOpen && 'rotate-180')} />
            </button>
            <AnimatePresence>
              {policyOpen && (
                <motion.div initial={{ opacity: 0, height: 0 }} animate={{ opacity: 1, height: 'auto' }} exit={{ opacity: 0, height: 0 }} className="overflow-hidden">
                  <div className="pt-4" data-testid="policy-panel"><PolicyEditor value={policy} onChange={setPolicy} prefix="s6pol" /></div>
                </motion.div>
              )}
            </AnimatePresence>
          </Card>
        </div>

        <div className="min-[1760px]:sticky min-[1760px]:top-4 min-[1760px]:self-start">
          <Card data-testid="summary" className="p-5">
            <Caps rules>{t('s6.summary')}</Caps>
            <dl className="mt-3 space-y-2 text-[13px]">
              <div className="flex justify-between gap-3"><dt className="text-[var(--muted)]">{t('role.author')}</dt><dd data-testid="sum-author" className="font-medium">{t(`provider.${author}`)}</dd></div>
              <div className="flex justify-between gap-3"><dt className="text-[var(--muted)]">{t('role.reviewer')}</dt><dd data-testid="sum-reviewer" className="font-medium">{t(`provider.${other(author)}`)}</dd></div>
            </dl>
            <ul data-testid="sum-policy" className="mt-3 space-y-1 border-t border-[var(--line)] pt-3 text-[12.5px] text-[var(--ink-2)]">
              {policySummary.map((s) => <li key={s}>· {s}</li>)}
            </ul>
            {missing.length > 0 && (
              <ul data-testid="missing" className="mt-3 space-y-0.5 text-[12px] text-[var(--value-ink)]">
                {missing.map((m) => <li key={m}>— {m}</li>)}
              </ul>
            )}
            <div className="mt-4 flex flex-wrap gap-2">
              <Button data-testid="cancel" onClick={() => nav('/reviews')}>{t('common.cancel')}</Button>
              <Button data-testid="start" variant="primary" disabled={missing.length > 0} onClick={start} className="flex-1 justify-center"><Rocket size={14} />{t('s6.start')}</Button>
            </div>
          </Card>
        </div>
      </div>
    </Page>
  )
}
