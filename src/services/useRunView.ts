// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { useMemo } from 'react'
import { runView } from '../lib/runView'
import type { Run } from '../state/store'
import { useRunData } from './useRunData'
/** The run's view model: the real snapshot on the desktop, the canned sample data in the browser. */
export function useRunView(run: Run | undefined) {
  const { snapshot } = useRunData(run?.id ?? '')
  return useMemo(() => run ? runView(run, snapshot) : null, [run, snapshot])
}
