// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Minimal product on the design system: the frame with one screen, a banner, a KPI row, a table and the agent panel.
// Everything product-specific — wordmark, roles, navigation, texts, banner — is configuration given to the package.
import { StrictMode, Suspense, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { createBrowserRouter, Navigate, Outlet, RouterProvider, useLocation } from 'react-router'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import * as Tooltip from '@radix-ui/react-tooltip'
import { useTranslation } from 'react-i18next'
import { FileText } from 'lucide-react'
import '@symvoulio/ui/fonts'
import './index.css'
import {
  AgentButton, AgentPanel, AllScreensPage, AppFrame, applyInitialTheme, Badge, Card, CountLink, EmptyState, Kpi, Money, PageBanner,
  PrototypeControls, ScreenPage, Table, TopBarSearch, TopBarSpacer, useThemeToggle, Wordmark,
} from '@symvoulio/ui'
import { fmt, getLanguages, initI18n, PSEUDO, setLanguage } from '@symvoulio/ui/i18n'

const THEME_KEY = 'example.theme'
const ROLES = ['accountant', 'partner'] as const

function Frame() {
  const { t, i18n } = useTranslation()
  const location = useLocation()
  const [role, setRole] = useState<string>('accountant')
  const [query, setQuery] = useState('')
  const [dark, toggleTheme] = useThemeToggle(THEME_KEY)
  const [agentOpen, setAgentOpen] = useState(false)
  return (
    <AppFrame scrollKey={location.pathname} breadcrumbsLabel={t('frame.breadcrumbs')}
      topBar={<>
        <Wordmark to="/home">EXAMPLE</Wordmark>
        <TopBarSearch label={t('frame.search')} placeholder={t('frame.searchShort')} value={query} onChange={setQuery} onSubmit={() => {}} />
        <TopBarSpacer />
        <CountLink to="/home" icon={<FileText size={16} aria-hidden />} label={t('frame.myQueue')} count={fmt.num(3)} />
        <AgentButton label={t('frame.agent')} onClick={() => setAgentOpen(true)} />
        <PrototypeControls title={t('frame.prototypeOnly')}
          role={{ label: t('frame.role'), value: role, onChange: setRole, options: ROLES.map((r) => ({ value: r, label: t(`role.${r}`) })) }}
          language={{ label: t('frame.language'), value: i18n.language, onChange: (l) => void setLanguage(l),
            options: [...getLanguages().map((l) => ({ value: l.code, label: l.native })), { value: PSEUDO, label: t('lang.pseudo') }] }}
          theme={{ label: t('frame.theme'), dark, onToggle: toggleTheme }} />
      </>}
      nav={{
        label: t('frame.navigation'),
        roleCaption: t('frame.sectionsFor', { role: t(`role.${role}`) }),
        items: [{ id: 'home', to: '/home', label: t('nav.home'), badge: <Badge tone="flag">{fmt.num(1)}</Badge> }],
        allScreens: { to: '/screens', label: t('frame.allScreens'), count: fmt.num(1) },
      }}
      overlays={<AgentPanel open={agentOpen} onOpenChange={setAgentOpen} title={t('frame.agent')} description={t(`role.${role}`)}
        closeLabel={t('common.close')} tryLabel={t('a1.try')} actions={[]} log={[]} placeholder={t('a1.placeholder')} sendLabel={t('a1.send')} onSend={() => {}} />}>
      <Outlet />
    </AppFrame>
  )
}

function Home() {
  const { t } = useTranslation('common')
  const rows = [{ client: 'Acme Ltd', due: '2026-11-15', fee: 1200 }, { client: 'Borealis LLC', due: '2026-11-30', fee: 860 }]
  return (
    <ScreenPage id="home" caption={t('home.caption')} title={t('home.title')} banner={<PageBanner screen="home" />}>
      <div className="grid grid-cols-2 gap-4">
        <Kpi label={t('home.fees')} value={fmt.money(2060)} why={t('home.feesWhy')} tone="value" />
        <Kpi label={t('home.docs')} value={fmt.num(7)} why={t('home.docsWhy')} />
      </div>
      <Card className="mt-4 p-4">
        <Table head={[t('home.client'), t('home.due'), t('home.fees')]}>
          {rows.map((r) => (
            <tr key={r.client}><td><bdi>{r.client}</bdi></td><td>{fmt.date(r.due)}</td><td><Money value={r.fee} why={t('home.feesWhy')} /></td></tr>
          ))}
        </Table>
        <p className="mt-3 text-[12px] text-[var(--muted)]">{t('home.clients', { count: rows.length })}</p>
      </Card>
      <EmptyState className="mt-4">{t('home.empty')}</EmptyState>
    </ScreenPage>
  )
}

function Screens() {
  const { t } = useTranslation()
  return <AllScreensPage caption={t('all.caption', { n: fmt.num(1) })} title={t('frame.allScreens')} intro={t('all.intro', { role: t('role.accountant') })} openLabel={t('all.open')}
    screens={[{ id: 'home', to: '/home', title: t('home.title'), caption: t('home.caption'), where: 'inMenu', whereLabel: t('all.inMenu') }]} />
}

const router = createBrowserRouter([{
  element: <Frame />,
  children: [
    { index: true, element: <Navigate to="/home" replace /> },
    { path: '/home', element: <Home /> },
    { path: '/screens', element: <Screens /> },
  ],
}])

applyInitialTheme(THEME_KEY)
await initI18n({ storageKey: 'example.lang', currency: 'USD' })

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <QueryClientProvider client={new QueryClient()}>
      <Tooltip.Provider>
        <Suspense fallback={null}>
          <RouterProvider router={router} />
        </Suspense>
      </Tooltip.Provider>
    </QueryClientProvider>
  </StrictMode>,
)
