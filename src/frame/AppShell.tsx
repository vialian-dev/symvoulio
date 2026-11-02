// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// The frame in the shared UI kit style: top bar with the wordmark and its mark (or the empty
// mark slot until the brand files are in), the light / dark toggle at the end of the bar as in the shared UI kit, the status strip «Round N of M · state», the
// left navigation as icons with labels, the content, and the two provider panels at about 38 % of the width. The
// agreement line stays visible on every run screen.
import { useLayoutEffect, useRef, useState, type PointerEvent as RPointerEvent, type ReactNode } from 'react'
import { NavLink, useLocation } from 'react-router'
import { useTranslation } from 'react-i18next'
import { FilePlus2, Info, LibraryBig, Moon, Settings, Sun } from 'lucide-react'
import { useThemeToggle } from '@symvoulio/ui/theme'
import { Wordmark, type BrandMark } from '@symvoulio/ui/frame'
import { THEME_KEY } from './theme'
import { cx } from '@symvoulio/ui/components'
import { ProviderPanels, useRunFromRoute } from './Panels'
import { useRunData } from '../services/useRunData'
import type { RunSnapshot } from '../services/types'
import { wasStopped } from '../lib/runState'
import { shownRound, useStore, type Run } from '../state/store'

const STATE_TONE: Record<string, string> = {
  failed: 'bg-[var(--flag)]', idle: 'bg-[var(--muted)]', running: 'bg-[var(--ok)] dot-pulse', paused: 'bg-[var(--value)]', stopped: 'bg-[var(--flag)]', completed: 'bg-[var(--accent)]',
}
/** Status strip states are idle, running, paused, stopped and completed; after a human decision the strip keeps how the run ended: Stopped or
 *  Completed (a run stopped in round 1 and then rejected is not «Completed»). */
export function stripState(r: Run | undefined, snapshot?: RunSnapshot): 'idle' | 'running' | 'paused' | 'stopped' | 'completed' | 'failed' {
  if (!r) return 'idle'
  if (r.state === 'failed') return 'failed'
  if (r.state === 'running' || r.state === 'paused' || r.state === 'stopped') return r.state
  if (r.state === 'returned') return 'paused'
  return wasStopped(r, snapshot) ? 'stopped' : 'completed'
}

function StatusStrip() {
  const { t } = useTranslation()
  const routeRun = useRunFromRoute()
  const runningRun = useStore((s) => s.runs.find((r) => r.state === 'running'))
  const run = routeRun ?? runningRun
  const { snapshot } = useRunData(run?.id ?? '')
  const st = stripState(run, snapshot)
  return (
    <div data-testid="status-strip" data-state={st} className="flex items-center gap-2 rounded-full border border-[var(--line)] bg-[var(--surface)] px-3 py-1 text-[12.5px]">
      <span className={cx('size-2 rounded-full', STATE_TONE[st])} aria-hidden />
      {run && st !== 'failed' ? <span className="num text-[14px] text-[var(--ink)]">{t('frame.round', { n: shownRound(run), m: run.policy.maxRounds })}</span> : null}
      {run && st !== 'failed' ? <span className="text-[var(--muted)]">·</span> : null}
      <span data-testid="status-state" className="font-medium text-[var(--ink-2)]">{t(`state.${st}`)}</span>
      {run && !routeRun ? <span className="max-w-[220px] truncate text-[var(--muted)]">— {run.name}</span> : null}
    </div>
  )
}

function NavItem({ to, icon, label, testId, match }: { to: string; icon: ReactNode; label: string; testId: string; match?: (p: string) => boolean }) {
  const { pathname } = useLocation()
  return (
    <NavLink to={to} data-testid={testId}
      className={({ isActive }) => cx('flex w-full flex-col items-center gap-1 rounded-xl px-1 py-2.5 text-center text-[11px] leading-tight font-medium transition-colors',
        isActive || match?.(pathname) ? 'bg-[var(--accent-soft)] text-[var(--accent)]' : 'text-[var(--ink-2)] hover:bg-[var(--surface-2)] hover:text-[var(--ink)]')}>
      {icon}<span>{label}</span>
    </NavLink>
  )
}

/** Brand mark, concept 5: the deep tonality on the light theme,
 *  the light tonality on the dark theme; 64 px at 1x, 180 px for 2x. Used only when the files are in public/brand/. */
const BRAND_MARK: BrandMark | undefined = __BRAND_MARK__ ? {
  onLight: { src: '/brand/symvoulio_mark_deep_64.png', srcSet: '/brand/symvoulio_mark_deep_64.png 1x, /brand/symvoulio_mark_deep_180.png 2x' },
  onDark: { src: '/brand/symvoulio_mark_light_64.png', srcSet: '/brand/symvoulio_mark_light_64.png 1x, /brand/symvoulio_mark_light_180.png 2x' },
} : undefined

/** Light / dark, in the place and form of the shared UI kit: a quiet dashed box at the end of the top bar. */
function ThemeToggle() {
  const { t } = useTranslation()
  const [dark, toggle] = useThemeToggle(THEME_KEY)
  return (
    <div className="ms-1 flex items-center rounded-lg border border-dashed border-[var(--line)] px-1.5 py-1" title={t('frame.theme')}>
      <button type="button" data-testid="theme-toggle" aria-label={t('frame.theme')} aria-pressed={dark} onClick={toggle}
        className="rounded p-1 text-[var(--muted)] hover:text-[var(--accent)]">
        {dark ? <Sun size={14} /> : <Moon size={14} />}
      </button>
    </div>
  )
}

export function AppShell({ children }: { children: ReactNode }) {
  const { t } = useTranslation()
  const { pathname } = useLocation()
  const onRunScreen = /^\/review\//.test(pathname)
  const [pct, setPct] = useState(38)
  const row = useRef<HTMLDivElement>(null)
  const main = useRef<HTMLElement>(null)
  // A new screen opens at its top (the S8 drawer and filters keep the pathname, so they keep the scroll).
  useLayoutEffect(() => { main.current?.scrollTo({ top: 0 }) }, [pathname])

  const drag = (e: RPointerEvent<HTMLDivElement>) => {
    const el = row.current
    if (!el) return
    e.currentTarget.setPointerCapture(e.pointerId)
    const move = (ev: PointerEvent) => {
      const r = el.getBoundingClientRect()
      setPct(Math.min(55, Math.max(26, ((r.right - ev.clientX) / r.width) * 100)))
    }
    const up = () => { window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up) }
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', up)
  }

  return (
    <div className="grid h-full grid-rows-[60px_minmax(0,1fr)]">
      <header data-testid="titlebar" className="flex min-w-0 items-center gap-3 overflow-hidden whitespace-nowrap border-b border-[var(--line)] bg-[color-mix(in_oklab,var(--surface)_82%,transparent)] px-4 backdrop-blur">
        <div className="flex shrink-0 items-center gap-2">
          {/* Until the brand files are in public/brand/, an empty slot keeps the mark's place. */}
          {!BRAND_MARK && <span data-testid="mark-slot" aria-hidden className="size-[30px] rounded-lg border border-dashed border-[var(--line)]" />}
          <Wordmark to="/reviews" mark={BRAND_MARK}>Symvoulio</Wordmark>
        </div>
        <div className="flex-1" />
        {onRunScreen && (
          <p data-testid="agreement-line" className="truncate text-[12.5px] text-[var(--ink-2)] italic">{t('frame.agreement')}</p>
        )}
        <StatusStrip />
        <ThemeToggle />
      </header>
      <div ref={row} className="flex min-h-0 min-w-0">
        <nav aria-label={t('nav.label')} data-testid="nav" className="flex w-[84px] shrink-0 flex-col items-stretch gap-1 border-e border-[var(--line)] px-2 py-3">
          <NavItem to="/reviews" testId="nav-reviews" icon={<LibraryBig size={20} />} label={t('nav.reviews')} match={(p) => p.startsWith('/review/')} />
          <NavItem to="/new" testId="nav-new" icon={<FilePlus2 size={20} />} label={t('nav.new')} />
          <div className="flex-1" />
          <NavItem to="/settings" testId="nav-settings" icon={<Settings size={20} />} label={t('nav.settings')} match={(p) => p.startsWith('/setup')} />
          <NavItem to="/about" testId="nav-about" icon={<Info size={20} />} label={t('nav.about')} />
          <p data-testid="copyright-frame" className="mt-2 text-center text-[10px] leading-tight text-[var(--muted)]">{t('frame.copyright')}</p>
        </nav>
        <main ref={main} data-testid="main" className="min-w-0 flex-1 overflow-y-auto">
          {children}
        </main>
        <div role="separator" aria-orientation="vertical" aria-label={t('panel.widthSplitter')} data-testid="width-splitter" onPointerDown={drag}
          className="group hidden w-2 shrink-0 cursor-col-resize items-center justify-center min-[1280px]:flex">
          <span className="h-12 w-1 rounded-full bg-[var(--line)] group-hover:bg-[var(--accent)]" />
        </div>
        <aside aria-label={t('panel.area')} data-testid="panels-area" style={{ width: `${pct}%` }} className="min-h-0 shrink-0 max-[1279.98px]:!w-[40%] max-[600px]:hidden">
          <ProviderPanels />
        </aside>
      </div>
    </div>
  )
}
