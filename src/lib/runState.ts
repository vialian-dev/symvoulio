// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// One reading of a run's end state for every screen: the status strip, S7, S9 and S10 must never disagree.
import { gateAfter } from './review'
import type { RunSnapshot } from '../services/types'
import type { Run } from '../state/store'

/** A policy gate ends automatic review rather than interrupting it. */
export function finalHumanGate(snapshot?: RunSnapshot): boolean {
  return snapshot?.stopReason === 'gate_human_review_required' && snapshot.roundsDone > 0
}

/** The run ended stopped (by the user, or by the gate with the threshold failed), even if a human decided afterwards. */
export function wasStopped(r: Run, snapshot?: RunSnapshot, desktop = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window): boolean {
  if (!r.sample && (desktop || snapshot)) {
    if (finalHumanGate(snapshot)) return false
    const lastGate = snapshot?.rounds.at(-1)?.gate
    return r.state === 'stopped' || snapshot?.stopReason === 'user_stop' || snapshot?.stopReason === 'cancelled' || snapshot?.stopReason === 'gate_human_review_required' || lastGate?.kind === 'stopped'
  }
  return r.state === 'stopped' || r.stopReason === 'user' || (r.done > 0 && gateAfter(r.done, r.policy).kind === 'stopped')
}

/** The run is over (stopped, completed or decided): its package can be exported — partial when it was stopped. */
export function canExport(r: Run): boolean {
  return r.state === 'stopped' || r.state === 'completed' || r.state === 'approved' || r.state === 'rejected'
}

/** The round a run has reached: a run stopped in the middle of a round has reached that round (its revision exists). */
export function reachedRound(r: Run): number {
  return r.done + (r.state === 'stopped' && r.step > 0 ? 1 : 0)
}
/** «Return for another round»: not once the run has reached ITS OWN maximum of
 *  rounds, whether it completed that round or was stopped in the middle of it. */
export function canReturn(r: Run): boolean {
  return reachedRound(r) < r.policy.maxRounds
}
