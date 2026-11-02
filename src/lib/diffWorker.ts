// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import type { BlockDiff } from './diff'

/** Termination alone cannot retract an already queued worker callback. */
export function runDiffWorker(worker: Pick<Worker, 'onmessage' | 'onerror' | 'postMessage' | 'terminate'>, input: unknown, apply: (diff: BlockDiff[]) => void, fail: (message: string) => void) {
  let cancelled = false
  const cleanup = () => { cancelled = true; worker.terminate() }
  worker.onmessage = event => {
    if (cancelled) return
    cleanup()
    apply(event.data)
  }
  worker.onerror = event => {
    if (cancelled) return
    cleanup()
    fail(event.message)
  }
  worker.postMessage(input)
  return cleanup
}
