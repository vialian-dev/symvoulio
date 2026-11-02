// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Row R47 (React update loop on the pending review page). Pre-fix commit f6f2cd9, fix commit 493a629. Old-API red variant.
// The screens use `view.doc(rev)` as an effect dependency. While the first desktop snapshot is pending, `runView` must
// return the SAME array reference on every call; a fresh `[]` per call changes the dependency on each render and loops
// (React "Maximum update depth exceeded"). Browser-free: uses only `runView(run, snapshot, desktop)` from
// src/lib/runView.ts, so it also runs at f6f2cd9, where `doc` returns a new `[]` each call (`=> []` / `?? []`).
// Run from the repo root: npx jiti tests/regression_variants/R47_pending_doc_reference_stable.check.ts
import assert from 'node:assert/strict'
import { runView } from '../../src/lib/runView.ts'
import { DEFAULT_POLICY } from '../../src/lib/review.ts'
import { toPolicy } from '../../src/services/policy.ts'
import type { Run } from '../../src/state/store.ts'
import type { RunSnapshot } from '../../src/services/types.ts'

const run = { policy: DEFAULT_POLICY, issueDecisions: {} } as Run
const noDocuments = { documents: [], issues: [], rounds: [], policy: toPolicy(DEFAULT_POLICY) } as unknown as RunSnapshot
const cases: Array<[string, ReturnType<typeof runView>]> = [
  ['pending desktop snapshot (undefined)', runView(run, undefined, true)],
  ['desktop snapshot with no documents', runView(run, noDocuments, true)],
]
for (const [name, view] of cases) {
  for (const rev of [-1, 0, 1]) {
    assert.ok(view.doc(rev) === view.doc(rev), `${name}: doc(${rev}) returns a new array on every call, an unstable effect dependency`)
  }
}
console.log('R47 pending doc references are stable: passed')
