// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Light / dark theme: the `dark` class on <html> switches every semantic variable of theme.css. The choice is remembered in
// localStorage under the product's key; `?theme=dark` forces dark for one load (screenshots, tests).
import { useState } from 'react'

/** Call once before the first render (no flash of the wrong theme). */
export function applyInitialTheme(storageKey: string) {
  try { if (localStorage.getItem(storageKey) === 'dark') document.documentElement.classList.add('dark') } catch { /* storage blocked */ }
  if (new URLSearchParams(location.search).get('theme') === 'dark') document.documentElement.classList.add('dark')
}

/** Current theme and its toggle, remembered under `storageKey`. */
export function useThemeToggle(storageKey: string): [dark: boolean, toggle: () => void] {
  const [dark, setDark] = useState(() => document.documentElement.classList.contains('dark'))
  const toggle = () => {
    const next = !dark
    setDark(next)
    document.documentElement.classList.toggle('dark', next)
    try { localStorage.setItem(storageKey, next ? 'dark' : 'light') } catch { /* storage blocked */ }
  }
  return [dark, toggle]
}
