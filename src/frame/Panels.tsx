// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// The right area of the frame: two provider terminal panels, always visible, stacked, with an adjustable
// splitter between them; double-click on a header maximises that panel until the next double-click. Headers name the
// provider and its role, never the shell. Below 1280 px the two panels become tabs.
// Maximised (Figma S07m): the other panel folds to its header bar (a click restores both), and the maximised panel says
// «Double-click to restore» in its header.
import { matchesRun } from '../state/store'
import { useEffect, useMemo, useRef, useState, type PointerEvent as RPointerEvent } from 'react'
import { useLocation } from 'react-router'
import { useTranslation } from 'react-i18next'
import { Maximize2, Minimize2 } from 'lucide-react'
import { cx } from '@symvoulio/ui/components'
import { Segmented } from '../components/ui/controls'
import { PROVIDERS, type Provider } from '../data/sample'
import { runLines, type TermLine } from '../lib/script'
import { activeProvider, pressEnter, promptFor, useStore, type Run } from '../state/store'
import { isDesktop } from '../services/env'
import { useRunData } from '../services/useRunData'
import { setRawOutput } from '../services/backend'
import { Terminal } from './Terminal'

// A run is a read-only log. Keep logical lines in the DOM when the viewport wraps them,
// so copying and assistive technology receive the original complete event.
function ProviderLog({ lines, raw, label, testId }: { lines: TermLine[]; raw: boolean; label: string; testId: string }) {
  const log = useRef<HTMLPreElement>(null)
  useEffect(() => { if (log.current) log.current.scrollTop = log.current.scrollHeight }, [lines, raw])
  return <pre ref={log} role="log" aria-label={label} data-testid={testId}
    className="h-full min-h-0 overflow-auto whitespace-pre-wrap break-all bg-[var(--term-bg)] p-3 font-[family-name:var(--font-mono)] text-[12.5px] leading-[1.25] text-[var(--term-ink)]">
    {lines.map(line => raw ? line.raw : line.readable).filter(Boolean).join('\n')}
  </pre>
}

let sessionRaw = false
export type PanelStatus = 'idle' | 'working' | 'error'

export function useRunFromRoute(): Run | undefined {
  const { pathname } = useLocation()
  const m = /^\/review\/([^/]+)/.exec(pathname)
  return useStore((s) => (m ? s.runs.find((r) => matchesRun(r, m[1])) : undefined))
}

function useWide() {
  const q = '(min-width: 1280px)'
  const [wide, setWide] = useState(() => window.matchMedia(q).matches)
  useEffect(() => {
    const m = window.matchMedia(q)
    const on = () => setWide(m.matches)
    m.addEventListener('change', on)
    return () => m.removeEventListener('change', on)
  }, [])
  return wide
}

export function ProviderPanels() {
  const { t } = useTranslation()
  const run = useRunFromRoute()
  const author = useStore((s) => (run ? run.author : s.author))
  const shells = useStore((s) => s.shells)
  useStore((s) => s.workspace) // the prompt shows the workspace path
  const [mode, setMode] = useState<Record<Provider, 'readable' | 'raw'>>({ claude: 'readable', codex: 'readable' })
  const [raw, setRaw] = useState(sessionRaw)
  const [rawError, setRawError] = useState('')
  const [rawPending, setRawPending] = useState(false)
  const [split, setSplit] = useState(0.5)
  const [max, setMax] = useState<Provider | null>(null)
  const [tab, setTab] = useState<Provider>('claude')
  const wide = useWide()
  const box = useRef<HTMLDivElement>(null)

  // Desktop app: a real run streams its own lines and names the provider at work; the sample review stays canned.
  const data = useRunData(run?.id ?? '')
  const real = isDesktop && !!run && !run.sample
  const rawFold = real && <details className="shrink-0 text-xs">
    <summary>{t('panel.rawOutput')}</summary>
    <label><input type="checkbox" checked={raw} disabled={rawPending} onChange={async e => {
      const enabled = e.target.checked
      const previous = raw
      setRaw(enabled)
      setRawPending(true)
      try {
        await setRawOutput(enabled)
        sessionRaw = enabled
        setRaw(enabled)
        setRawError('')
      } catch (error) { setRaw(previous); setRawError(String(error)) }
      finally { setRawPending(false) }
    }} />{t('panel.showRawOutput')}</label>
    {rawError && <p role="alert">{rawError}</p>}
  </details>
  const active = real ? (run.state === 'running' ? data.provider ?? run.author : null) : activeProvider(run)
  const lines = useMemo<Record<Provider, TermLine[]>>(() => {
    if (!run) return { claude: shells.claude.lines, codex: shells.codex.lines }
    if (real) return { claude: data.lines?.claude ?? [], codex: data.lines?.codex ?? [] }
    const out = {} as Record<Provider, TermLine[]>
    for (const p of PROVIDERS) {
      const l = runLines(p, run.author, run.done, run.step, run.document.name)
      if (run.error?.provider === p) l.push({ readable: t('panel.authFailed'), raw: '{"type":"system","subtype":"api_retry","attempt":1,"error_status":401,"error":"authentication_failed"}', tone: 'error' })
      out[p] = l
    }
    return out
  }, [shells, t, run?.id, run?.author, run?.done, run?.step, run?.document.name, run?.error?.provider, real, data.lines])

  const status = (p: Provider): PanelStatus => (run?.error?.provider === p ? 'error' : active === p ? 'working' : 'idle')
  const roleOf = (p: Provider) => (p === author ? 'author' : 'reviewer')

  const onDrag = (e: RPointerEvent<HTMLDivElement>) => {
    const el = box.current
    if (!el) return
    e.currentTarget.setPointerCapture(e.pointerId)
    const move = (ev: PointerEvent) => {
      const r = el.getBoundingClientRect()
      setSplit(Math.min(0.85, Math.max(0.15, (ev.clientY - r.top) / r.height)))
    }
    const up = () => { window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up) }
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', up)
  }

  const panel = (p: Provider, style?: React.CSSProperties) => (
    <section key={p} data-testid={`panel-${p}`} data-status={status(p)} data-role={roleOf(p)} style={style}
      aria-label={`${t(`provider.${p}`)} · ${t(`role.${roleOf(p)}`)}`}
      className={cx('flex min-h-0 flex-col overflow-hidden rounded-xl border border-[var(--term-line)] bg-[var(--term-bg)]', status(p) === 'working' && 'panel-pulse', status(p) === 'error' && 'border-[var(--flag)]')}>
      <header data-testid={`panel-head-${p}`} onDoubleClick={() => setMax((m) => (m === p ? null : p))} title={t('panel.dblclick')}
        className="@container flex shrink-0 cursor-default items-center gap-2 border-b border-[var(--term-line)] px-3 py-2 select-none">
        <span data-testid={`dot-${p}`} data-status={status(p)} aria-label={t(`panel.status.${status(p)}`)} role="img"
          className={cx('size-2 shrink-0 rounded-full', status(p) === 'idle' && 'bg-[var(--mist-500,#6a7f9c)]', status(p) === 'working' && 'dot-pulse bg-[var(--ok)]', status(p) === 'error' && 'bg-[var(--flag)]')} />
        {/* provider and role are never cut; at a narrow panel the title wraps between them and the status word
            gives way to the dot (which keeps it as its label). */}
        <h2 data-testid={`panel-title-${p}`} className="caps min-w-0 !text-[11px] leading-tight !text-[var(--ink-2)] @max-[520px]:!tracking-[0.1em]">
          <span className="whitespace-nowrap">{t(`provider.${p}`).toUpperCase()} ·</span> <span>{t(`role.${roleOf(p)}`).toUpperCase()}</span>
        </h2>
        <span className="shrink-0 text-[11px] text-[var(--muted)] @max-[460px]:hidden">{t(`panel.status.${status(p)}`)}</span>
        <span className="flex-1" />
        {max === p && <span data-testid={`restore-hint-${p}`} className="shrink-0 rounded-full bg-[var(--accent-soft)] px-2 py-0.5 text-[11px] whitespace-nowrap text-[var(--accent)] @max-[500px]:hidden">{t('panel.dblRestore')}</span>}
        {!real && <Segmented size="sm" label={t('panel.output')} value={mode[p]} testId={`mode-${p}`} onChange={(v) => setMode((m) => ({ ...m, [p]: v }))}
          options={[{ value: 'readable', label: t('panel.readable'), testId: `mode-${p}-readable` }, { value: 'raw', label: t('panel.raw'), testId: `mode-${p}-raw` }]} />}
        {wide && (
          <button type="button" aria-label={max === p ? t('panel.restore') : t('panel.maximise')} data-testid={`max-${p}`} onClick={() => setMax((m) => (m === p ? null : p))}
            className="rounded p-1 text-[var(--muted)] hover:text-[var(--ink)]">{max === p ? <Minimize2 size={13} /> : <Maximize2 size={13} />}</button>
        )}
      </header>
      <div className="min-h-0 flex-1">
        {real ? <ProviderLog lines={lines[p]} raw={raw} label={`${t(`provider.${p}`)} · ${t(`role.${roleOf(p)}`)}`} testId={`term-${p}`} /> :
        <Terminal contextKey={run ? `run:${run.id}` : 'shell'} lines={lines[p]} mode={real ? raw ? 'raw' : 'readable' : mode[p]} testId={`term-${p}`}
          label={`${t(`provider.${p}`)} · ${t(`role.${roleOf(p)}`)}`}
          prompt={run ? null : promptFor(p)} pending={run ? null : shells[p].pending} onEnter={run ? undefined : () => pressEnter(p)}
          live={isDesktop && !run ? p : undefined} />}
      </div>
    </section>
  )

  if (!wide) {
    return (
      <div className="flex h-full min-h-0 flex-col gap-2 p-3" data-testid="panels" data-layout="tabs">
        {rawFold}
        <div role="tablist" aria-label={t('panel.tabs')} className="flex shrink-0 gap-1">
          {PROVIDERS.map((p) => (
            <button key={p} role="tab" type="button" aria-selected={tab === p} data-testid={`tab-${p}`} onClick={() => setTab(p)}
              className={cx('flex items-center gap-1.5 rounded-lg px-2.5 py-1.5 text-[11px] font-medium tracking-wide', tab === p ? 'bg-[var(--accent-soft)] text-[var(--accent)]' : 'text-[var(--muted)] hover:text-[var(--ink)]')}>
              <span className={cx('size-1.5 rounded-full', status(p) === 'working' ? 'bg-[var(--ok)]' : status(p) === 'error' ? 'bg-[var(--flag)]' : 'bg-[var(--muted)]')} />
              {t(`provider.${p}`).toUpperCase()} · {t(`role.${roleOf(p)}`).toUpperCase()}
            </button>
          ))}
        </div>
        <div className="min-h-0 flex-1">{panel(tab, { height: '100%' })}</div>
      </div>
    )
  }

  /** The other panel while one is maximised: only its header bar; a click restores both. */
  const folded = (p: Provider) => (
    <button key={`f-${p}`} type="button" data-testid={`panel-folded-${p}`} onClick={() => setMax(null)} title={t('panel.restore')}
      className="flex shrink-0 items-center gap-2 rounded-xl border border-[var(--term-line)] bg-[var(--term-bg)] px-3 py-2 text-start select-none hover:border-[var(--accent)]">
      <span data-status={status(p)} aria-hidden className={cx('size-2 shrink-0 rounded-full', status(p) === 'idle' && 'bg-[var(--mist-500,#6a7f9c)]', status(p) === 'working' && 'dot-pulse bg-[var(--ok)]', status(p) === 'error' && 'bg-[var(--flag)]')} />
      <span className="caps min-w-0 !text-[11px] leading-tight !text-[var(--ink-2)]"><span className="whitespace-nowrap">{t(`provider.${p}`).toUpperCase()} ·</span> <span className="whitespace-nowrap">{t(`role.${roleOf(p)}`).toUpperCase()}</span></span>
      <span className="text-[11px] text-[var(--muted)]">{t(`panel.status.${status(p)}`)}</span>
      <span className="flex-1" />
      <Minimize2 size={13} className="text-[var(--muted)]" />
    </button>
  )
  const top = max === 'claude' ? 1 : max === 'codex' ? 0 : split
  return (
    <div ref={box} className="flex h-full min-h-0 flex-col p-3" data-testid="panels" data-layout="stacked" data-max={max ?? ''}>
      {rawFold}
      {max === 'codex' && folded('claude')}
      {max === 'codex' && <div className="h-2 shrink-0" />}
      {top > 0 && panel('claude', { flexBasis: `${top * 100}%`, flexGrow: max === 'claude' ? 1 : 0, flexShrink: 1 })}
      {max === null && (
        <div role="separator" aria-orientation="horizontal" aria-label={t('panel.splitter')} aria-valuenow={Math.round(split * 100)} aria-valuemin={15} aria-valuemax={85} tabIndex={0}
          data-testid="splitter" onPointerDown={onDrag}
          onKeyDown={(e) => { if (e.key === 'ArrowUp') setSplit((s) => Math.max(0.15, s - 0.05)); if (e.key === 'ArrowDown') setSplit((s) => Math.min(0.85, s + 0.05)) }}
          className="group flex h-3 shrink-0 cursor-row-resize items-center justify-center focus-visible:outline-none">
          <span className="h-1 w-12 rounded-full bg-[var(--line)] transition-colors group-hover:bg-[var(--accent)] group-focus-visible:bg-[var(--accent)]" />
        </div>
      )}
      {top < 1 && panel('codex', { flexBasis: `${(1 - top) * 100}%`, flexGrow: 1, flexShrink: 1 })}
      {max === 'claude' && <div className="h-2 shrink-0" />}
      {max === 'claude' && folded('codex')}
    </div>
  )
}
