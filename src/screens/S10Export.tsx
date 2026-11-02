// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { matchesRun } from '../state/store'
// S10 Export package: a tree preview of output/ exactly as in the spec, with this run's revisions and
// reviews; Export and Open folder (stubs), Export partial only for a stopped run.
import { useState, type ReactNode } from 'react'
import { useNavigate, useParams } from 'react-router'
import { useTranslation } from 'react-i18next'
import { motion } from 'motion/react'
import { ArrowLeft, CheckCircle2, FileDown, FileJson, FileText, Folder, FolderOpen, Globe, PackageOpen } from 'lucide-react'
import { Button, Card } from '@symvoulio/ui/components'
import { Page } from '../components/Page'
import { useStore } from '../state/store'
import { wasStopped } from '../lib/runState'
import { backend } from '../services/backend'
import { isDesktop } from '../services/env'
import { useRunData } from '../services/useRunData'

interface Node { name: string; kind: 'dir' | 'file'; children?: Node[] }
const pad = (n: number) => String(n).padStart(2, '0')

export function outputTree(done: number, docName: string): Node {
  const dir = (name: string, children: Node[] = []): Node => ({ name, kind: 'dir', children })
  const file = (name: string): Node => ({ name, kind: 'file' })
  const rounds = Array.from({ length: done }, (_, i) => i + 1)
  return dir('output', [
    dir('original', [file(docName)]),
    dir('revisions', rounds.map((n) => file(`revision_${pad(n)}.docx`))),
    dir('reviews', rounds.map((n) => file(`review_${pad(n)}.json`))),
    dir('final', ['final_document.docx', 'final_document.md', 'final_review.html', 'unresolved_issues.json', 'human_decision.json'].map(file)),
    dir('audit', [file('run_manifest.json'), file('timeline.json'), dir('provider_output')]),
  ])
}

/** The real package on the desktop: relative paths from the backend, as one tree under output/. */
export function treeFromPaths(paths: string[]): Node {
  const root: Node = { name: 'output', kind: 'dir', children: [] }
  for (const path of paths) {
    let at = root
    const parts = path.split('/')
    parts.forEach((name, i) => {
      const kind = i === parts.length - 1 ? 'file' : 'dir'
      let next = at.children!.find((c) => c.name === name && c.kind === kind)
      if (!next) { next = kind === 'dir' ? { name, kind, children: [] } : { name, kind }; at.children!.push(next) }
      at = next
    })
  }
  return root
}
/** Windows verbatim prefix (\\?\) is noise for a person reading the path. */
const plainPath = (p: string) => p.replace(/^\\\\\?\\/, '')

function icon(n: Node): ReactNode {
  if (n.kind === 'dir') return n.children?.length ? <FolderOpen size={15} className="text-[var(--accent)]" /> : <Folder size={15} className="text-[var(--accent)]" />
  if (n.name.endsWith('.json')) return <FileJson size={15} className="text-[var(--value)]" />
  if (n.name.endsWith('.html')) return <Globe size={15} className="text-[var(--muted)]" />
  return <FileText size={15} className="text-[var(--ink-2)]" />
}

function Tree({ node, depth = 0 }: { node: Node; depth?: number }) {
  return (
    <li>
      <motion.div initial={{ opacity: 0, x: -4 }} animate={{ opacity: 1, x: 0 }} transition={{ delay: depth * 0.05 }}
        data-testid={`node-${node.name}`} className="flex items-center gap-2 rounded-md px-2 py-1 font-[family-name:var(--font-mono)] text-[12.5px] hover:bg-[var(--surface-2)]">
        {icon(node)}<span className={node.kind === 'dir' ? 'text-[var(--ink)]' : 'text-[var(--ink-2)]'}>{node.name}{node.kind === 'dir' ? '/' : ''}</span>
      </motion.div>
      {node.children?.length ? (
        <ul className="ms-[15px] border-s border-[var(--line)] ps-3">{node.children.map((c) => <Tree key={c.name} node={c} depth={depth + 1} />)}</ul>
      ) : null}
    </li>
  )
}

export function ExportPackage() {
  const { t } = useTranslation()
  const nav = useNavigate()
  const { id } = useParams()
  const run = useStore((s) => s.runs.find((r) => matchesRun(r, id)))
  const workspace = useStore((s) => s.workspace)
  const [done, setDone] = useState<string | null>(null)
  const { snapshot, error } = useRunData(id ?? '')
  const [exported, setExported] = useState<{ path: string; tree: string[] } | null>(null)
  if (!run) return <Page id="S10" caption={t('s10.caption')} title={t('s7.missing')}><Button onClick={() => nav('/reviews')}>{t('nav.reviews')}</Button></Page>

  // Desktop: the real package (the snapshot's tree, refreshed by Export) and the real folder; browser: canned.
  const real = isDesktop && !run.sample && !!snapshot
  const unavailable = isDesktop && !real
  // The store may know a desktop run by a local alias until start returns; the backend knows it by snapshot.id.
  const runId = real ? snapshot.id : run.id
  const path = exported ? plainPath(exported.path) : `${workspace}\\runs\\${runId}\\output`
  const fail = (e: unknown) => setDone(String(e))
  const doExport = (partial: boolean) => backend.exportRun(runId, partial).then((r) => {
    setExported(r)
    setDone(partial ? t('s10.partialDone', { n: run.done }) : t('s10.exported', { path: plainPath(r.path) }))
  }, fail)
  const stopped = wasStopped(run, snapshot) // the same reading as S7 and S9
  return (
    <Page id="S10" caption={t('s10.caption')} title={t('s10.title')} accent={t('s10.accent')}
      actions={<Button data-testid="back-to-workspace" onClick={() => nav(`/review/${run.id}`)}><ArrowLeft size={14} />{t('s9.backToWorkspace')}</Button>}>
      <p className="-mt-2 mb-4 text-[13px] text-[var(--muted)]">{run.name}</p>
      <div className="grid gap-4 min-[1600px]:grid-cols-[minmax(0,1fr)_340px]">
        <Card className="p-5">
          <ul data-testid="tree"><Tree node={real ? treeFromPaths(exported?.tree ?? snapshot.outputTree) : outputTree(run.done, run.document.name)} /></ul>
        </Card>
        <Card className="space-y-3 self-start p-5">
          {unavailable && !run.sample && error != null
            ? <p data-testid="export-load-error" role="alert" className="text-[13px] text-[var(--flag)]">{String(error instanceof Error ? error.message : error)}</p>
            : <p className="text-[13px] text-[var(--ink-2)]">{t(unavailable ? run.sample ? 's10.sampleUnavailable' : 'service.loading' : 's10.where')}</p>}
          <code data-testid="run-path" className="block rounded-lg bg-[var(--term-bg)] px-3 py-2 font-[family-name:var(--font-mono)] text-[11.5px] break-all text-[var(--ink-2)]">{path}</code>
          <div className="flex flex-wrap gap-2">
            <Button data-testid="do-export" disabled={unavailable} variant="primary" onClick={() => (real ? void doExport(false) : setDone(t('s10.exported', { path })))}><FileDown size={14} />{t('s10.export')}</Button>
            <Button data-testid="open-folder" disabled={unavailable} onClick={() => (real ? void backend.openFolder(path).then(() => setDone(t('s10.opened')), fail) : setDone(t('s10.opened')))}><FolderOpen size={14} />{t('s10.openFolder')}</Button>
            <Button data-testid="export-partial" disabled={unavailable || !stopped} title={stopped ? undefined : t('s10.partialDisabled')} onClick={() => (real ? void doExport(true) : setDone(t('s10.partialDone', { n: run.done })))}><PackageOpen size={14} />{t('s10.partial')}</Button>
          </div>
          {!stopped && <p className="text-[12px] text-[var(--muted)]">{t('s10.partialDisabled')}</p>}
          {done && <p data-testid="export-result" role="status" className="flex items-start gap-1.5 text-[12.5px] text-[var(--ok)]"><CheckCircle2 size={14} className="mt-0.5 shrink-0" />{done}</p>}
          {unavailable && run.sample && <p data-testid="sample-export-unavailable" role="status">{t('s10.sampleUnavailable')}</p>}
          {!real && !unavailable && <p className="border-t border-[var(--line)] pt-3 text-[12px] text-[var(--muted)]">{t('s10.stub')}</p>}
        </Card>
      </div>
    </Page>
  )
}
