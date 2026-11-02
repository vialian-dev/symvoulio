// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// R51: canned Return -> Pause -> Resume must match the desktop engine. In crates/symvoulio-core/src/engine.rs the limit
// `returned_until` is set once on resume from Returned (`s.returned_until = Some(s.snapshot.rounds_done + 1)`) and cleared ONLY
// when the run ends (`let returned_done = s.returned_until.is_some_and(|n| s.snapshot.rounds_done >= n)` ... `s.returned_until = None;
// state(&mut s, value, ...)`), never by Pause. So Return -> Pause -> Resume runs exactly one more round and ends Completed
// (Stopped when the last gate stopped), as an uninterrupted Return does. Stop, approve and reject still clear the limit.
import assert from 'node:assert/strict'
import { STEPS_PER_ROUND } from '../lib/script'

Object.defineProperty(globalThis, 'location', { value: { search: '' }, configurable: true })
const callbacks = new Map<number, () => void>()
let serial = 0
Object.defineProperty(globalThis, 'window', { value: {
  setTimeout: (callback: () => void) => { callbacks.set(++serial, callback); return serial },
}, configurable: true })
Object.defineProperty(globalThis, 'clearTimeout', { value: (id: number) => callbacks.delete(id), configurable: true })
const { runById, decide, stopRun, pauseRun, resumeRun } = await import('./store')
const run = () => runById('sample')!

function returned() {
  callbacks.clear()
  Object.assign(run(), { done: 1, step: 0, state: 'stopped', returnedUntil: undefined })
  decide('sample', 'returned', 'Reviewer', 'One more round')
  assert.equal(run().returnedUntil, 2)
}
function runToEnd(limit = 3 * STEPS_PER_ROUND) {
  for (let i = 0; i < limit && callbacks.size > 0; i++) {
    const [id, callback] = callbacks.entries().next().value!
    callbacks.delete(id)
    callback()
  }
}
function step(n: number) {
  for (let i = 0; i < n; i++) {
    const [id, callback] = callbacks.entries().next().value!
    callbacks.delete(id)
    callback()
  }
}

// Return -> Pause (at once, and after some steps) -> Resume: exactly one extra round, then Completed.
for (const stepsBeforePause of [0, 2]) {
  returned()
  step(stepsBeforePause)
  pauseRun('sample')
  assert.equal(run().state, 'paused')
  assert.equal(callbacks.size, 0, 'a paused run schedules nothing')
  resumeRun('sample')
  runToEnd()
  assert.equal(run().done, 2, `Return -> Pause(after ${stepsBeforePause} steps) -> Resume: round count (desktop: 2)`)
  assert.equal(run().state, 'completed', `Return -> Pause(after ${stepsBeforePause} steps) -> Resume: final state (desktop: completed)`)
  assert.equal(run().returnedUntil, undefined, 'the limit is cleared when the run ends')
  assert.equal(callbacks.size, 0, 'nothing is left scheduled after the extra round')
}

// Control: an uninterrupted Return gives the same outcome.
returned()
runToEnd()
assert.equal(run().done, 2)
assert.equal(run().state, 'completed')

// Still clearing: Stop (Resume then continues past the old limit), approve and reject, also after a Pause.
returned()
pauseRun('sample')
stopRun('sample')
assert.equal(run().returnedUntil, undefined, 'Stop clears the Return limit')
resumeRun('sample')
step(STEPS_PER_ROUND)
assert.equal(run().state, 'running', 'Return -> Pause -> Stop -> Resume continues beyond the old limit')
for (const kind of ['approved', 'rejected'] as const) {
  returned()
  pauseRun('sample')
  decide('sample', kind, 'Reviewer', '')
  assert.equal(run().returnedUntil, undefined, `${kind} after Pause clears the Return limit`)
  stopRun('sample')
}
console.log('r51 check passed: Return -> Pause -> Resume ends after one extra round, Completed')
