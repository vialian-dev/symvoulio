// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import '@fontsource/inter/400.css'
import '@fontsource/inter/500.css'
import '@fontsource/inter/600.css'
import '@fontsource/cormorant-sc/500.css'
import '@fontsource/cormorant-sc/600.css'
import '@fontsource/cormorant-sc/700.css'
import '@fontsource/jetbrains-mono/400.css'
import '@xterm/xterm/css/xterm.css'
import './index.css'
import { applyInitialTheme } from '@symvoulio/ui/theme'
import { initI18n } from './i18n'
import { THEME_KEY } from './frame/theme'
import App from './App'

// Light by default, as the shared UI kit; dark stays one
// click away in the top bar. The choice is remembered; `?theme=dark` forces dark for one load (tests, screenshots).
applyInitialTheme(THEME_KEY)

await initI18n()

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
