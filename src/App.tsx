// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { BrowserRouter, Navigate, Route, Routes, useLocation } from 'react-router'
import * as Tooltip from '@radix-ui/react-tooltip'
import type { ReactNode } from 'react'
import { AppShell } from './frame/AppShell'
import { ErrorBoundary } from './components/ErrorBoundary'
import { useStore } from './state/store'
import { SystemCheck } from './screens/S1SystemCheck'
import { Connect } from './screens/S2S3Connect'
import { Roles } from './screens/S4Roles'
import { PolicyScreen } from './screens/S5Policy'
import { NewReview } from './screens/S6NewReview'
import { Workspace } from './screens/S7Workspace'
import { Decision } from './screens/S9Decision'
import { ExportPackage } from './screens/S10Export'
import { Reviews } from './screens/S11Reviews'
import { SettingsScreen } from './screens/S12Settings'
import { About } from './screens/S13About'

/** First launch (map 0.2): until setup is finished every address leads into the wizard. */
function Guard({ children }: { children: ReactNode }) {
  const done = useStore((s) => s.setupDone)
  const { pathname } = useLocation()
  if (!done && !pathname.startsWith('/setup')) return <Navigate to="/setup/system" replace />
  return <>{children}</>
}

export default function App() {
  return (
    <ErrorBoundary>
    <BrowserRouter>
      <Tooltip.Provider delayDuration={200}>
        <AppShell>
          <Guard>
            <Routes>
              <Route path="/" element={<Navigate to="/reviews" replace />} />
              <Route path="/setup/system" element={<SystemCheck />} />
              <Route path="/setup/claude" element={<Connect provider="claude" />} />
              <Route path="/setup/codex" element={<Connect provider="codex" />} />
              <Route path="/setup/roles" element={<Roles />} />
              <Route path="/setup/policy" element={<PolicyScreen />} />
              <Route path="/reviews" element={<Reviews />} />
              <Route path="/new" element={<NewReview />} />
              <Route path="/review/:id" element={<Workspace />} />
              <Route path="/review/:id/decision" element={<Decision />} />
              <Route path="/review/:id/export" element={<ExportPackage />} />
              <Route path="/settings" element={<SettingsScreen />} />
              <Route path="/about" element={<About />} />
              <Route path="*" element={<Navigate to="/reviews" replace />} />
            </Routes>
          </Guard>
        </AppShell>
      </Tooltip.Provider>
    </BrowserRouter>
    </ErrorBoundary>
  )
}
