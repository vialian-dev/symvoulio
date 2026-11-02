// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import type { RunSnapshot } from '../services/types'
export function humanQuestions(snapshot?: RunSnapshot) {
 return (snapshot?.humanQuestions ?? []).map(q => ({ id: q.issue_id, text: q.question, answeredBy: q.answeredBy, answeredAt: q.answeredAt, resolved: q.resolved, answer: (q as typeof q & { answer?: string | null }).answer ?? null })).sort((a, b) => Number(a.resolved) - Number(b.resolved))
}
