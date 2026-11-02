// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import type { Policy as ViewPolicy } from '../lib/review'
import type { Policy } from './types'
export function toPolicy(p: ViewPolicy): Policy {
  return { max_rounds: p.maxRounds, min_rounds: p.minRounds, contribution: p.contribution, new_content_limit_pct: p.newContent === 'custom' ? p.customPct : Number(p.newContent), stop: { no_critical: p.stop.noCritical, no_high: p.stop.noHigh, no_new: p.stop.noNew, max_rounds: p.stop.maxRounds } }
}
export function viewPolicy(p: Policy): ViewPolicy {
  const n = p.new_content_limit_pct
  return { maxRounds: p.max_rounds, minRounds: p.min_rounds, contribution: p.contribution, newContent: n === 0 || n === 10 || n === 25 ? String(n) as '0' | '10' | '25' : 'custom', customPct: n, stop: { noCritical: p.stop.no_critical, noHigh: p.stop.no_high, noNew: p.stop.no_new, maxRounds: p.stop.max_rounds } }
}
