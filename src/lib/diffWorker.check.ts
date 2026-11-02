// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
/// <reference types="node" />
import assert from 'node:assert/strict'
import { runDiffWorker } from './diffWorker'

function fakeWorker() {
  return { onmessage: null, onerror: null, postMessage() {}, terminate() {} } as Pick<Worker, 'onmessage' | 'onerror' | 'postMessage' | 'terminate'>
}
const applied: unknown[] = [], errors: string[] = []
const old = fakeWorker()
const cleanup = runDiffWorker(old, {}, diff => applied.push(diff), message => errors.push(message))
// Capture queued callbacks before cleanup, just as the browser event queue can.
const lateMessage = old.onmessage!, lateError = old.onerror!
cleanup()
const current = fakeWorker()
runDiffWorker(current, {}, diff => applied.push(diff), message => errors.push(message))
current.onmessage!.call(current as Worker, { data: ['current'] } as MessageEvent)
lateMessage.call(old as Worker, { data: ['cancelled'] } as MessageEvent)
lateError.call(old as Worker, { message: 'cancelled failure' } as ErrorEvent)
assert.deepEqual(applied, [['current']], 'cancelled or superseded diff must never apply')
assert.deepEqual(errors, [], 'cancelled worker errors must never apply')
console.log('R18 queued worker callbacks after cleanup ignored')
