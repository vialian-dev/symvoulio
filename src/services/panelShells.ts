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
