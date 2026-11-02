// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { useStore } from '../state/store'
import { runLines, runProviders, serviceErrors, snapshots } from './storeAdapter'
/** Subscribe to store notifications, including terminal events with no Run change. */
export function useRunData(id: string) {
  useStore(s => s)
  return { snapshot: snapshots.get(id), lines: runLines.get(id), provider: runProviders.get(id), error: serviceErrors.get(id) }
}
