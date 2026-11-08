// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { useSyncExternalStore } from 'react'
import type { Provider } from './types'
let shells: Partial<Record<Provider, string>> = {}
const listeners = new Set<() => void>()
export function setPanelShell(panel: Provider, shell?: string) {
  shells = { ...shells, [panel]: shell }
  listeners.forEach(listener => listener())
}
export function usePanelShell(panel: Provider) {
  return useSyncExternalStore(listener => { listeners.add(listener); return () => { listeners.delete(listener) } }, () => shells[panel])
}

export const getPanelShell = (panel: Provider) => shells[panel]

// F2/F4: the live terminal of a panel registers how to give it a clean shell (close the PTY, open a fresh one).
// `resetPanel` resolves once the fresh shell is attached; false when no terminal of that panel is on screen.
const resetters: Partial<Record<Provider, () => Promise<void>>> = {}
export function registerPanelReset(panel: Provider, reset: () => Promise<void>) {
  resetters[panel] = reset
  return () => { if (resetters[panel] === reset) delete resetters[panel] }
}
export function resetPanel(panel: Provider): Promise<boolean> {
  const reset = resetters[panel]
  return reset ? reset().then(() => true) : Promise.resolve(false)
}
