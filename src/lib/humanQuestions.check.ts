// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Failing before fix: Error: Cannot find module './humanQuestions'
// Wire regression before fix: actual ['done', 'open']; expected ['open', 'done']
/// <reference types="node" />
import assert from 'node:assert/strict'
import * as display from './humanQuestions'
assert.equal(typeof display.humanQuestions, 'function', 'R10 human questions must reach the decision view')
const questions = display.humanQuestions({ humanQuestions: [{ issue_id: 'done', question: 'Done?', resolved: true }, { issue_id: 'open', question: 'Choose?', resolved: false }] } as never)
assert.deepEqual(questions.map(q => q.id), ['open', 'done'])
assert.equal(display.humanQuestions(undefined).length, 0)
console.log('R10 unresolved questions first passed')

const answered = display.humanQuestions({ humanQuestions: [{ issue_id: 'unique-id', question: 'Budget owner?', resolved: true, answer: 'Finance', answeredBy: 'Dana', answeredAt: '2026-10-31T06:00:00Z' }] } as never)[0]
assert.deepEqual(answered, { id: 'unique-id', text: 'Budget owner?', resolved: true, answer: 'Finance', answeredBy: 'Dana', answeredAt: '2026-10-31T06:00:00Z' })
console.log('R34 question, answer, answerer and time preserved for display passed')
