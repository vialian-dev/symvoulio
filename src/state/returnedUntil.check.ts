// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import assert from 'node:assert/strict'
import { STEPS_PER_ROUND } from '../lib/script'

// Drive the real canned scheduler without wall-clock waits or a browser.
Object.defineProperty(globalThis, 'location', { value: { search: '' }, configurable: true })
const callbacks = new Map<number, () => void>()
let serial = 0
Object.defineProperty(globalThis, 'window', { value: {
  setTimeout: (callback: () => void) => { callbacks.set(++serial, callback); return serial },
}, configurable: true })
Object.defineProperty(globalThis, 'clearTimeout', { value: (id: number) => callbacks.delete(id), configurable: true })
const { runById, decide, stopRun, pauseRun, resumeRun } = await import('./store')
function returned() {
  Object.assign(runById('sample')!, { done: 1, step: 0, state: 'stopped', returnedUntil: undefined })
  decide('sample', 'returned', 'Reviewer', 'One more round')
  assert.equal(runById('sample')!.returnedUntil, 2)
}
function finishRound() {
  for (let step = runById('sample')!.step; step < STEPS_PER_ROUND; step++) {
    const [id, callback] = callbacks.entries().next().value!
    callbacks.delete(id)
    callback()
  }
}
returned()
finishRound()
assert.equal(runById('sample')!.state, 'completed', 'An uninterrupted Return still stops after one round')
assert.equal(runById('sample')!.returnedUntil, undefined)
returned()
stopRun('sample')
assert.equal(runById('sample')!.returnedUntil, undefined, 'Stop must clear the Return limit before Resume')
resumeRun('sample')
finishRound()
assert.equal(runById('sample')!.done, 2)
assert.equal(runById('sample')!.state, 'running', 'Return → Stop → Resume must continue beyond the old round limit')
stopRun('sample')
returned()
pauseRun('sample')
// R51: the desktop engine keeps the Return's one-round limit through Pause (engine.rs clears returned_until only when the
// limit is reached or the run ends), so Pause must keep it; the old assertion here asserted the opposite, i.e. the defect.
assert.equal(runById('sample')!.returnedUntil, 2, 'Pause keeps the Return limit (see r51.check.ts for the later Resume)')
for (const kind of ['approved', 'rejected'] as const) {
  returned()
  decide('sample', kind, 'Reviewer', '')
  assert.equal(runById('sample')!.returnedUntil, undefined, `${kind} clears the Return limit`)
  stopRun('sample')
}
console.log('returnedUntil check passed: Return → Stop → Resume, Pause, approve, reject')
