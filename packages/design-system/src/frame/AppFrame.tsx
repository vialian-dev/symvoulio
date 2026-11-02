// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// The common frame of every screen: top bar, left navigation, breadcrumbs, content column.
// What the product puts in it — wordmark, project switch, counters, navigation items, roles — comes in by props; the
// frame holds no product data and no locale keys.
//
// Responsive rule of the top bar: no wrap at >= 1280 px (Tailwind `xl`); below it labels of the mode
// pill and My queue collapse to their icons and the name stays in aria-label; the search field shows from `lg`.
// RTL: logical properties only (ms/me/ps/pe/start/end, border-e); arrows and chevrons flip with `rtl:rotate-180`.
import { createContext, useContext, useLayoutEffect, useRef, useState, type ComponentPropsWithoutRef, type ReactNode, type Ref } from 'react'
import { Link, NavLink, useLocation, type LinkProps } from 'react-router'
import { Sparkles as AgentMark, ChevronRight, Flag, LayoutGrid, Languages, Menu, Moon, Search, Sun, User, X } from 'lucide-react'
import { Badge, Button, Caps } from '../components/ui'
import { cx } from '../cx'

// ---------- breadcrumbs ----------
/** Breadcrumbs: Company › Project › Cycle › Task › Step. Screens pass their trail with useCrumbs. */
export interface Crumb { label: string; to?: string }
const CrumbCtx = createContext<(c: Crumb[]) => void>(() => {})
// eslint-disable-next-line react-refresh/only-export-components
export function useCrumbs() { return useContext(CrumbCtx) }

export function Breadcrumbs({ crumbs, label }: { crumbs: Crumb[]; label: string }) {
  if (crumbs.length === 0) return null
  return (
    <nav aria-label={label}><ol data-testid="crumbs" className="mb-3 flex flex-wrap items-center gap-1 text-[12px] text-[var(--muted)]">
      {crumbs.map((c, i) => (
        <li key={i} className="flex items-center gap-1">
          {i > 0 && <ChevronRight size={12} aria-hidden className="rtl:rotate-180" />}
          {c.to ? <Link to={c.to} className="hover:text-[var(--accent)]"><bdi>{c.label}</bdi></Link> : <bdi aria-current="page" className="text-[var(--ink-2)]">{c.label}</bdi>}
        </li>
      ))}
    </ol></nav>
  )
}

// ---------- the frame ----------
export interface AppFrameProps {
  /** Contents of the top bar, left to right (Wordmark, TopBarSelect, TopBarSearch, ModePill, TopBarSpacer, …). */
  topBar: ReactNode
  /** The left navigation. */
  nav: SideNavProps
  /** Below 1024 px (useNarrow): controls the top bar gives up (project, search, mode, role, language, theme), shown at the
   *  top of the navigation drawer. */
  drawerExtras?: ReactNode
  /** aria-labels of the drawer's menu and close buttons. */
  menuLabel?: string
  closeLabel?: string
  /** aria-label of the breadcrumbs. */
  breadcrumbsLabel: string
  /** Each new value scrolls the content back to its top (pass the route pathname). */
  scrollKey?: string
  /** The screen (router <Outlet/>). */
  children: ReactNode
  /** Panels and dialogs that live beside the grid (agent panel, preview dialog); they see useCrumbs too. */
  overlays?: ReactNode
}

/** Below 1024 px the frame is narrow: the navigation becomes a drawer and the top bar keeps only its essentials.
 *  A media query, not CSS alone, so each control exists once in the DOM at any width. */
export function useNarrow() {
  const q = '(max-width: 1023.98px)'
  const [narrow, setNarrow] = useState(() => typeof window !== 'undefined' && window.matchMedia(q).matches)
  useLayoutEffect(() => {
    const m = window.matchMedia(q)
    const on = () => setNarrow(m.matches)
    on(); m.addEventListener('change', on)
    return () => m.removeEventListener('change', on)
  }, [])
  return narrow
}

export function AppFrame({ topBar, nav, breadcrumbsLabel, scrollKey, children, overlays, drawerExtras, menuLabel = 'Menu', closeLabel = 'Close' }: AppFrameProps) {
  const [crumbs, setCrumbs] = useState<Crumb[]>([])
  const mainRef = useRef<HTMLElement>(null)
  // A layout effect: it runs before a screen's own effects, so a deep link (S07 ?tab=history) still scrolls to its target.
  useLayoutEffect(() => { mainRef.current?.scrollTo({ top: 0 }); window.scrollTo({ top: 0 }) }, [scrollKey])
  return (
    <CrumbCtx.Provider value={setCrumbs}>
      <FrameGrid topBar={topBar} nav={nav} mainRef={mainRef} drawerExtras={drawerExtras} menuLabel={menuLabel} closeLabel={closeLabel} scrollKey={scrollKey}>
        <Breadcrumbs crumbs={crumbs} label={breadcrumbsLabel} />
        {children}
      </FrameGrid>
      {overlays}
    </CrumbCtx.Provider>
  )
}

/** The grid: 232 px navigation, 60 px top bar, content column up to 1360 px. */
function FrameGrid({ topBar, nav, mainRef, children, drawerExtras, menuLabel, closeLabel, scrollKey }: {
  topBar: ReactNode; nav: SideNavProps; mainRef: Ref<HTMLElement>; children: ReactNode; drawerExtras?: ReactNode; menuLabel: string; closeLabel: string; scrollKey?: string
}) {
  const narrow = useNarrow()
  const [open, setOpen] = useState(false)
  // A newly opened screen closes the drawer; so does leaving the narrow layout.
  useLayoutEffect(() => { setOpen(false) }, [scrollKey, narrow])
  if (!narrow) return (
    <div className="grid h-full grid-cols-[232px_1fr] grid-rows-[60px_1fr]">
      {/* ---------- top bar ---------- */}
      <TopBar>{topBar}</TopBar>
      {/* ---------- left navigation ---------- */}
      <SideNav {...nav} />
      {/* ---------- content ---------- */}
      <main ref={mainRef} data-testid="main" className="overflow-y-auto">
        <div className="mx-auto max-w-[1360px] px-8 pt-4 pb-16">
          {children}
        </div>
      </main>
    </div>
  )
  // Narrow (< 1024 px): one column; the navigation is a drawer behind the menu button; content takes the full width.
  return (
    <div className="grid h-full grid-cols-[minmax(0,1fr)] grid-rows-[60px_1fr]">
      <TopBar>
        {/* A 44 × 44 tap target. */}
        <button type="button" data-testid="menu" aria-label={menuLabel} aria-expanded={open} onClick={() => setOpen((o) => !o)}
          className="grid size-11 shrink-0 place-items-center rounded-lg text-[var(--ink-2)] hover:bg-[var(--surface-2)]"><Menu size={20} aria-hidden /></button>
        {topBar}
      </TopBar>
      {open && <div aria-hidden className="fixed inset-0 top-[60px] z-30 bg-[var(--ink)]/20" onClick={() => setOpen(false)} />}
      {open && (
        <div data-testid="drawer" onClickCapture={(e) => { if ((e.target as Element).closest('a')) setOpen(false) }} className="fixed top-[60px] bottom-0 start-0 z-40 flex w-[min(300px,88vw)] flex-col overflow-y-auto border-e border-[var(--line)] bg-[var(--surface)] shadow-[var(--shadow-lift)]">
          <div className="flex items-start justify-between gap-2 px-3 pt-3">
            <div className="flex min-w-0 flex-1 flex-col items-start gap-2" data-testid="drawer-extras">{drawerExtras}</div>
            <button type="button" aria-label={closeLabel} onClick={() => setOpen(false)} className="rounded p-1 text-[var(--muted)] hover:bg-[var(--surface-2)]"><X size={16} aria-hidden /></button>
          </div>
          <SideNav {...nav} />
        </div>
      )}
      <main ref={mainRef} data-testid="main" className="min-w-0 overflow-y-auto">
        {/* Narrow content: every grid stacks into one shrinkable column and a card scrolls its own wide content
            (tables, heatmaps, diffs) and rows wrap, so neither the page nor the content pane ever scrolls sideways. */}
        <div className="mx-auto max-w-[1360px] px-4 pt-4 pb-16 [&_.grid]:grid-cols-[minmax(0,1fr)] [&_.grid>*]:min-w-0 [&_.flex]:flex-wrap [overflow-wrap:break-word] [&_.serif-title]:[overflow-wrap:anywhere] [&_.caps]:whitespace-normal">
          {children}
        </div>
      </main>
    </div>
  )
}

// ---------- top bar ----------
/**
 * The top bar never overflows: after layout it measures itself; while its content is wider than the bar it
 * steps down — level 1 hides the labels of the mode pill and the queue link (icons with aria-labels stay), level 2 also
 * the Agent button's label (icon with aria-label), level 3 hides the search field, level 4 makes the wordmark smaller,
 * level 5 tightens the gaps and level 6 shrinks the wordmark and the brand mark further (narrow phones); if the bar still
 * overflows, the brand mark is hidden (data-nomark). It re-measures on
 * resize and on any text change (language switch), always starting from the full bar.
 */
function TopBar({ children }: { children: ReactNode }) {
  const ref = useRef<HTMLElement>(null)
  useLayoutEffect(() => {
    const h = ref.current
    if (!h) return
    const fit = () => {
      delete h.dataset.nomark
      for (const level of ['0', '1', '2', '3', '4', '5', '6']) {
        h.dataset.compact = level
        if (h.scrollWidth <= h.clientWidth) return
      }
      // last step: the brand mark gives way (the name and every control stay) — e.g. Hebrew at 390 px
      h.dataset.nomark = '1'
    }
    fit()
    // Re-fit when the bar's width changes (window resize), its text changes (language switch, counts) or web fonts finish
    // loading. Only the bar itself is observed, and only a real width change counts: the fit's own level changes resize
    // the items, and observing them made a resize loop (eyes v10 run: pages hung at 390 px).
    let width = h.clientWidth
    const ro = new ResizeObserver(() => { if (h.clientWidth !== width) { width = h.clientWidth; fit() } })
    ro.observe(h)
    // An item that grows by itself (the language select showing a longer name, a late image) re-fits only when the bar now
    // overflows — a one-way step, so the fit's own changes can never start a loop.
    const grow = new ResizeObserver(() => { if (h.scrollWidth > h.clientWidth) fit() })
    for (const c of h.children) grow.observe(c)
    const mo = new MutationObserver(() => fit())
    mo.observe(h, { subtree: true, characterData: true, childList: true })
    void document.fonts?.ready.then(fit)
    // A font face requested later (a script the first render did not use, e.g. Hebrew) changes widths without a mutation.
    document.fonts?.addEventListener('loadingdone', fit)
    return () => { ro.disconnect(); grow.disconnect(); mo.disconnect(); document.fonts?.removeEventListener('loadingdone', fit) }
  }, [])
  return (
    <header ref={ref} data-testid="topbar" data-compact="0"
      className="group/bar col-span-2 flex min-w-0 items-center gap-3 overflow-hidden whitespace-nowrap data-[compact=5]:gap-1.5 data-[compact=5]:px-3 data-[compact=6]:gap-1 data-[compact=6]:px-2 border-b border-[var(--line)] bg-[color-mix(in_oklab,var(--surface)_82%,transparent)] px-4 backdrop-blur">
      {children}
    </header>
  )
}

// ---------- top bar parts ----------
/** One image of the brand mark: a square source and its srcset (e.g. the 64 and 180 px files as 1x / 2x+). */
export interface BrandMarkImage { src: string; srcSet?: string }
/** The product's brand mark, one image per theme: `onLight` for the light theme, `onDark` for the dark theme (`.dark`). */
export interface BrandMark { onLight: BrandMarkImage; onDark: BrandMarkImage }

/** Product name in serif small caps over a short accent rule; links home. `children` is the product's own wordmark text.
 *  `mark`: the product's brand mark, shown before the name (inline-start, so on the right in RTL) at 30 px — 26 / 20 px at
 *  the narrower top-bar levels, hidden only when the bar still overflows — with fixed width and height, so a late image never moves the bar. Decorative: the name
 *  is the link's text. */
export function Wordmark({ children, mark, ...link }: LinkProps & { children: ReactNode; mark?: BrandMark }) {
  const img = (m: BrandMarkImage, cls: string, theme: string) => (
    <img src={m.src} srcSet={m.srcSet} alt="" aria-hidden width={30} height={30} decoding="async" draggable={false} data-brand-mark={theme}
      className={cx('size-[30px] shrink-0 select-none object-contain group-data-[compact=4]/bar:size-[26px] group-data-[compact=5]/bar:size-[26px] group-data-[compact=6]/bar:size-[20px] group-data-[nomark]/bar:!hidden', cls)} />
  )
  return (
    <Link {...link} className="me-2 flex items-center gap-2 group-data-[compact=6]/bar:gap-1 group-data-[nomark]/bar:gap-0" data-testid="wordmark">
      {mark && <>{img(mark.onLight, 'dark:hidden', 'light')}{img(mark.onDark, 'hidden dark:block', 'dark')}</>}
      <span className="flex flex-col leading-none">
        <bdi dir="ltr" className="serif-title text-[19px] font-bold tracking-[0.06em] group-data-[compact=4]/bar:text-[15px] group-data-[compact=5]/bar:text-[15px] group-data-[compact=6]/bar:text-[12px]">{children}</bdi>
        <span className="mt-1 h-[2px] w-full rounded bg-[var(--accent)]" />
      </span>
    </Link>
  )
}

/** A compact select of the top bar (project switch) with a screen-reader label. */
export function TopBarSelect({ id, label, value, onChange, options }: {
  id: string; label: string; value: string; onChange: (v: string) => void; options: { value: string; label: string }[]
}) {
  return (
    <>
      <label className="sr-only" htmlFor={id}>{label}</label>
      <select id={id} value={value} onChange={(e) => onChange(e.target.value)}
        className="rounded-lg border border-[var(--line)] bg-[var(--surface)] px-2 py-1.5 text-[13px] text-[var(--ink-2)]">
        {options.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
      </select>
    </>
  )
}

/** Search field (from `lg`): the full hint in aria-label and title, a short one as placeholder. */
export function TopBarSearch({ label, placeholder, value, onChange, onSubmit, drawer }: {
  label: string; placeholder: string; value: string; onChange: (v: string) => void; onSubmit: (v: string) => void
  /** In the narrow layout's drawer: full width, always shown. */
  drawer?: boolean
}) {
  return (
    <form role="search" className={drawer ? 'relative block w-full' : 'relative ms-1 hidden w-72 min-w-[13rem] shrink lg:block group-data-[compact=3]/bar:!hidden group-data-[compact=4]/bar:!hidden group-data-[compact=5]/bar:!hidden group-data-[compact=6]/bar:!hidden'} onSubmit={(e) => { e.preventDefault(); onSubmit(value) }}>
      <Search size={15} aria-hidden className="pointer-events-none absolute top-2 start-2.5 text-[var(--muted)]" />
      <input data-testid="search" type="search" aria-label={label} title={label} placeholder={placeholder} value={value}
        onChange={(e) => onChange(e.target.value)}
        className="w-full rounded-lg border border-[var(--line)] bg-[var(--surface)] py-1.5 pe-2 ps-8 text-[13px] outline-none focus:border-[var(--accent)]" />
    </form>
  )
}

/** Mode pill: accent when `active`. No wrap at >= 1280 px; below it the pill collapses to its icon, name in aria-label. */
export function ModePill({ active, icon, label, ...link }: LinkProps & { active: boolean; icon: ReactNode; label: string }) {
  return (
    <Link {...link} aria-label={label}
      className={cx('inline-flex shrink-0 items-center gap-1.5 whitespace-nowrap rounded-full border px-2.5 py-1 text-[12px] font-medium group-data-[compact=6]/bar:px-1.5', active
        ? 'border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]' : 'border-[var(--line)] text-[var(--ink-2)]')}>
      {icon}
      <span data-label className="group-data-[compact=1]/bar:hidden group-data-[compact=2]/bar:hidden group-data-[compact=3]/bar:hidden group-data-[compact=4]/bar:hidden group-data-[compact=5]/bar:hidden group-data-[compact=6]/bar:hidden">{label}</span>
    </Link>
  )
}

/** Pushes the following top bar items to the end. */
export function TopBarSpacer() { return <div className="flex-1" /> }

/** A counted link («My queue 3»): icon, label from 1280 px, count badge; the full text with the count in aria-label. */
export function CountLink({ icon, label, count, countTestId, ...link }: LinkProps & { icon: ReactNode; label: string; count: string; countTestId?: string }) {
  return (
    <Link {...link} aria-label={`${label} (${count})`} className="flex shrink-0 items-center gap-1.5 whitespace-nowrap rounded-lg px-2 py-1.5 text-[13px] text-[var(--ink-2)] hover:bg-[var(--surface-2)] group-data-[compact=6]/bar:gap-1 group-data-[compact=6]/bar:px-1">
      {icon}<span data-label className="group-data-[compact=1]/bar:hidden group-data-[compact=2]/bar:hidden group-data-[compact=3]/bar:hidden group-data-[compact=4]/bar:hidden group-data-[compact=5]/bar:hidden group-data-[compact=6]/bar:hidden">{label}</span> <Badge tone="accent"><span data-testid={countTestId}>{count}</span></Badge>
    </Link>
  )
}

/** Red flag count: the flag, the number in serif, the label for screen readers. */
export function FlagCount({ label, count, ...link }: LinkProps & { label: string; count: string }) {
  return (
    <Link {...link} className="flex shrink-0 items-center gap-1 rounded-lg px-2 py-1.5 text-[13px] text-[var(--flag)] hover:bg-[var(--flag-soft)] group-data-[compact=6]/bar:px-1">
      <Flag size={15} aria-hidden /><span className="sr-only">{label}</span> <span className="num text-[15px] font-semibold">{count}</span>
    </Link>
  )
}

/** The Agent button: primary, with the agent mark. */
export function AgentButton({ label, ...rest }: ComponentPropsWithoutRef<'button'> & { label: string }) {
  // the label never wraps; when the bar cannot hold it, the fit logic collapses it to the icon (level 2+).
  return <Button variant="primary" aria-label={label} className="shrink-0 whitespace-nowrap" {...rest}><AgentMark size={15} /><span className="whitespace-nowrap group-data-[compact=2]/bar:hidden group-data-[compact=3]/bar:hidden group-data-[compact=4]/bar:hidden group-data-[compact=5]/bar:hidden group-data-[compact=6]/bar:hidden">{label}</span></Button>
}

/**
 * Prototype-only controls in a dashed box: role switch (to show rights), language, theme. Not product settings; a product
 * hides them in its snapshot mode.
 */
export function PrototypeControls({ title, role, language, theme }: {
  title: string
  role?: { label: string; value: string; options: { value: string; label: string }[]; onChange: (v: string) => void }
  language: { label: string; value: string; options: { value: string; label: string }[]; onChange: (v: string) => void }
  theme: { label: string; dark: boolean; onToggle: () => void }
}) {
  return (
    <div className="ms-1 flex items-center gap-1 rounded-lg border border-dashed border-[var(--line)] px-1.5 py-1" title={title}>
      {role && <>
        <User size={14} className="text-[var(--muted)]" />
        <select aria-label={role.label} data-testid="role" value={role.value} onChange={(e) => role.onChange(e.target.value)}
          className="bg-transparent text-[12px] text-[var(--ink-2)] outline-none">
          {role.options.map((r) => <option key={r.value} value={r.value}>{r.label}</option>)}
        </select>
      </>}
      <Languages size={14} className="ms-1 text-[var(--muted)]" />
      <select aria-label={language.label} data-testid="lang" value={language.value} onChange={(e) => language.onChange(e.target.value)}
        className="bg-transparent text-[12px] text-[var(--ink-2)] outline-none">
        {language.options.map((l) => <option key={l.value} value={l.value}>{l.label}</option>)}
      </select>
      <button type="button" aria-label={theme.label} aria-pressed={theme.dark} onClick={theme.onToggle} className="ms-1 rounded p-1 text-[var(--muted)] hover:text-[var(--accent)]">
        {theme.dark ? <Sun size={14} /> : <Moon size={14} />}
      </button>
    </div>
  )
}

// ---------- left navigation ----------
export interface NavItem {
  /** Stable id (data-nav). */
  id: string
  to: string
  label: ReactNode
  /** Also active while the pathname starts with this (e.g. '/project' for /project/:id). */
  activePrefix?: string
  /** Shown at the end of the row (e.g. a flag count badge). */
  badge?: ReactNode
  /** data-edge of the link (transition tests). */
  edge?: string
}
export interface SideNavProps {
  /** Caption above the list and the nav's aria-label («Navigation»). */
  label: string
  /** The role line, e.g. «Sections for role: COO». */
  roleCaption: ReactNode
  items: NavItem[]
  /** The «All screens» page: one link under the list, with the number of screens (only one in the nav — eyes v11 remark). */
  allScreens: { to: string; label: string; count: string }
}

export function SideNav({ label, roleCaption, items, allScreens }: SideNavProps) {
  const location = useLocation()
  return (
    <nav aria-label={label} className="overflow-y-auto border-e border-[var(--line)] px-3 py-4" data-testid="nav">
      <Caps className="px-2">{label}</Caps>
      {/* The menu depends on the role (items without the role's right are hidden); say so. Every screen stays one click away
          through the single «All screens» link under the list. */}
      <p data-testid="nav-role" className="px-2 pt-1 pb-2 text-[11.5px] leading-snug text-[var(--muted)]">{roleCaption}</p>
      {items.map((it) => (
        <NavLink key={it.id} to={it.to} data-edge={it.edge} data-nav={it.id}
          className={({ isActive }) => cx('flex items-center justify-between rounded-lg px-2.5 py-2 text-[13.5px]',
            isActive || (it.activePrefix !== undefined && location.pathname.startsWith(it.activePrefix))
              ? 'bg-[var(--accent-soft)] font-medium text-[var(--accent)]' : 'text-[var(--ink-2)] hover:bg-[var(--surface-2)]')}>
          <span>{it.label}</span>
          {it.badge ?? null}
        </NavLink>
      ))}
      <div className="my-2 border-t border-[var(--line)]" />
      <NavLink to={allScreens.to} data-nav="all" data-testid="nav-all-screens"
        className={({ isActive }) => cx('flex items-center gap-2 rounded-lg px-2.5 py-2 text-[13.5px]',
          isActive ? 'bg-[var(--accent-soft)] font-medium text-[var(--accent)]' : 'text-[var(--ink-2)] hover:bg-[var(--surface-2)]')}>
        <LayoutGrid size={14} aria-hidden />{allScreens.label} <span className="num ms-auto text-[12px] text-[var(--muted)]">{allScreens.count}</span>
      </NavLink>
    </nav>
  )
}
