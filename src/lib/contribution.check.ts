// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Failing before fix: AssertionError [ERR_ASSERTION]: R11 contribution gate must have a visible label
/// <reference types="node" />
import assert from 'node:assert/strict'
import { runView } from './runView'
import { DEFAULT_POLICY } from './review'
import fixture from '../services/wire.fixture.json'
import type { Run } from '../state/store'
import type { RunSnapshot } from '../services/types'
const snapshot = structuredClone(fixture.runSnapshot) as unknown as RunSnapshot
snapshot.rounds[0].gate.reason = 'continue_contribution'
assert.equal(runView({ policy: DEFAULT_POLICY, issueDecisions: {} } as Run, snapshot).gate(1)?.key, 'continueContribution', 'R11 contribution gate must have a visible label')
console.log('R11 contribution gate passed')
