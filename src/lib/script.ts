// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Canned provider output of the prototype: each round is 5 author steps then 5 reviewer steps. Readable
// lines are the normalized events of the review workflow; Raw lines imitate each CLI's own structured stream (Claude Code
// `-p --output-format stream-json`, Codex `exec --json`). No real process runs.
import { issuesAt, openCounts } from './review'
import type { Provider } from '../data/sample'

export const STEPS_PER_SIDE = 5
export const STEPS_PER_ROUND = STEPS_PER_SIDE * 2

export interface TermLine { readable: string; raw: string; tone?: 'event' | 'done' | 'error' | 'cmd' | 'out' }

const cmd: Record<Provider, (role: 'author' | 'reviewer') => string> = {
  claude: (role) => `claude -p --output-format stream-json --verbose --json-schema schemas/${role === 'author' ? 'author-response' : 'review'}.schema.json`,
  codex: (role) => `codex exec --json --sandbox read-only --output-schema schemas/${role === 'author' ? 'author-response' : 'review'}.schema.json -`,
}

function rawEvent(p: Provider, kind: 'init' | 'read' | 'work' | 'write' | 'result', round: number, detail: string): string {
  if (p === 'claude') {
    switch (kind) {
      case 'init': return `{"type":"system","subtype":"init","session_id":"c7e1-r${round}","tools":["Read"],"permissionMode":"default"}`
      case 'read': return `{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"${detail}"}}]}}`
      case 'work': return `{"type":"assistant","message":{"content":[{"type":"text","text":"${detail}"}]}}`
      case 'write': return `{"type":"assistant","message":{"content":[{"type":"text","text":"${detail}"}]}}`
      case 'result': return `{"type":"result","subtype":"success","is_error":false,"num_turns":${3 + round},"structured_output":{${detail}}}`
    }
  }
  switch (kind) {
    case 'init': return `{"type":"thread.started","thread_id":"0199-r${round}"}\n{"type":"turn.started"}`
    case 'read': return `{"type":"item.completed","item":{"type":"command_execution","command":"Get-Content ${detail}","exit_code":0}}`
    case 'work': return `{"type":"item.completed","item":{"type":"reasoning","text":"${detail}"}}`
    case 'write': return `{"type":"item.completed","item":{"type":"agent_message","text":"${detail}"}}`
    case 'result': return `{"type":"turn.completed","usage":{"input_tokens":${18000 + round * 2100},"cached_input_tokens":${9000 + round * 1500},"output_tokens":${2400 + round * 310}}}`
  }
}

const countText = (round: number) => {
  const c = openCounts(issuesAt(round))
  return `${c.critical} Critical, ${c.high} High, ${c.medium} Medium`
}

/** Step `i` (0-based within the round) of round `round`: which side speaks and what it prints. */
export function stepLine(round: number, i: number, author: Provider, docName: string): { side: 'author' | 'reviewer'; line: TermLine } {
  const reviewer: Provider = author === 'claude' ? 'codex' : 'claude'
  const rev = String(round).padStart(2, '0')
  if (i < STEPS_PER_SIDE) {
    const p = author
    const lines: TermLine[] = [
      { readable: `Round ${round} started`, raw: `> ${cmd[p]('author')}\n${rawEvent(p, 'init', round, '')}`, tone: 'event' },
      { readable: 'Reading source documents', raw: rawEvent(p, 'read', round, round === 1 ? `inputs/${docName}` : `revisions/revision_${String(round - 1).padStart(2, '0')}.md`), tone: 'event' },
      { readable: round === 1 ? 'Applying the objective and acceptance criteria' : 'Applying accepted findings', raw: rawEvent(p, 'work', round, round === 1 ? 'Objective and 5 criteria loaded' : `Findings to answer: review_${String(round - 1).padStart(2, '0')}.json`), tone: 'event' },
      { readable: `Creating revision ${round}`, raw: rawEvent(p, 'write', round, `Writing revision_${rev} and author_response_${rev}`), tone: 'event' },
      { readable: 'Completed', raw: rawEvent(p, 'result', round, `"revision":"revision_${rev}.md","author_response":"author_response_${rev}.json"`), tone: 'done' },
    ]
    return { side: 'author', line: lines[i] }
  }
  const p = reviewer
  const j = i - STEPS_PER_SIDE
  const lines: TermLine[] = [
    { readable: `Review ${round} started`, raw: `> ${cmd[p]('reviewer')}\n${rawEvent(p, 'init', round, '')}`, tone: 'event' },
    { readable: `Reading revision ${round} and the acceptance criteria`, raw: rawEvent(p, 'read', round, `revisions/revision_${rev}.md`), tone: 'event' },
    { readable: 'Checking claims against the sources', raw: rawEvent(p, 'work', round, 'Comparing figures with supporting files'), tone: 'event' },
    { readable: `Writing findings: ${countText(round)} open`, raw: rawEvent(p, 'write', round, `review_${rev}.json: ${countText(round)} open`), tone: 'event' },
    { readable: 'Completed', raw: rawEvent(p, 'result', round, ''), tone: 'done' },
  ]
  return { side: 'reviewer', line: lines[j] }
}

/** Every line printed so far for one provider: `done` full rounds and `step` steps of the current one. */
export function runLines(provider: Provider, author: Provider, done: number, step: number, docName: string): TermLine[] {
  const side = provider === author ? 'author' : 'reviewer'
  const out: TermLine[] = []
  for (let r = 1; r <= done + 1; r++) {
    const n = r <= done ? STEPS_PER_ROUND : step
    for (let i = 0; i < n; i++) {
      const s = stepLine(r, i, author, docName)
      if (s.side === side) out.push(s.line)
    }
  }
  return out
}
