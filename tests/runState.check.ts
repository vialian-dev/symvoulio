// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import assert from 'node:assert/strict'
import { finalHumanGate, wasStopped } from '../src/lib/runState'
import { DEFAULT_POLICY } from '../src/lib/review'
import type { Run } from '../src/state/store'
import type { RunSnapshot } from '../src/services/types'
const run = { done: 2, state: 'completed', policy: { ...DEFAULT_POLICY, maxRounds: 2 }, issueDecisions: {} } as Run
const snapshot = { stopReason: 'gate_clean', rounds: [{ round: 2, gate: { kind: 'stop', reason: 'stop_clean' } }] } as RunSnapshot
assert.equal(wasStopped(run, snapshot, true), false, 'desktop clean 2/2 must not be stopped')
assert.equal(wasStopped({ ...run, sample: true }, snapshot, true), wasStopped(run), 'sample is unchanged')
snapshot.stopReason = 'gate_max_rounds'
snapshot.rounds[0].gate.kind = 'stopped'
assert.equal(wasStopped(run, snapshot, true), true)
console.log('runState check passed')

snapshot.stopReason = 'gate_human_review_required'
snapshot.roundsDone = 3
assert.equal(finalHumanGate(snapshot), true)
for (const state of ['stopped', 'completed', 'approved', 'rejected'] as const) {
  assert.equal(wasStopped({ ...run, state }, snapshot, true), false, `final gate stays complete after ${state}`)
}
for (const reason of ['user_stop', 'cancelled'] as const) {
  snapshot.stopReason = reason
  assert.equal(finalHumanGate(snapshot), false)
  assert.equal(wasStopped({ ...run, state: 'rejected' }, snapshot, true), true, 'interrupted provenance survives rejection')
}
snapshot.stopReason = null
snapshot.rounds = []
assert.equal(wasStopped({ ...run, state: 'paused' }, snapshot, true), false)
console.log('final human gate and interrupted provenance checks passed')
