// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Small review pieces shared by S7, S8 and S9: severity dot, gate chip, document view, diff view.
import { useTranslation } from 'react-i18next'
import { CirclePause, CircleStop, CornerDownRight } from 'lucide-react'
import { cx } from '@symvoulio/ui/components'
import type { Block, Severity } from '../data/sample'
import type { Gate } from '../lib/review'
import type { BlockDiff } from '../lib/diff'

export const SEV_COLOR: Record<Severity, string> = {
  critical: 'var(--sev-critical)', high: 'var(--sev-high)', medium: 'var(--sev-medium)', low: 'var(--sev-low)',
}
export function SevDot({ s }: { s: Severity }) {
  return <span aria-hidden className="inline-block size-2 shrink-0 rounded-full" style={{ background: SEV_COLOR[s] }} />
}
export function SevLabel({ s }: { s: Severity }) {
  const { t } = useTranslation()
  return <span className="inline-flex items-center gap-1.5 text-[12px] font-medium" style={{ color: SEV_COLOR[s] }}><SevDot s={s} />{t(`sev.${s}`)}</span>
}

export function gateText(t: (k: string, o?: Record<string, unknown>) => string, g: Gate) {
  const parts: string[] = []
  if (g.counts.critical) parts.push(t('gate.nCritical', { count: g.counts.critical }))
  if (g.counts.high) parts.push(t('gate.nHigh', { count: g.counts.high }))
  return t(`gate.${g.key}`, { what: parts.join(', ') })
}

/** `past`: the verdict belongs to an earlier round while a new one runs: shown muted, «Round N: …». */
export function GateChip({ gate, testId, past }: { gate: Gate; testId?: string; past?: number }) {
  const { t } = useTranslation()
  const Icon = gate.kind === 'continue' ? CornerDownRight : gate.kind === 'stop' ? CirclePause : CircleStop
  return (
    <span data-testid={testId} data-kind={gate.kind} data-past={past ?? undefined}
      className={cx('inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-[12px] font-medium whitespace-nowrap',
        past !== undefined ? 'border-dashed border-[var(--line)] text-[var(--muted)]' : gate.kind === 'continue' && 'border-[var(--value)]/50 bg-[var(--value-soft)] text-[var(--value-ink)]',
        gate.kind === 'stop' && 'border-[var(--accent)]/50 bg-[var(--accent-soft)] text-[var(--accent)]',
        gate.kind === 'stopped' && 'border-[var(--flag)]/50 bg-[var(--flag-soft)] text-[var(--flag)]')}>
      <Icon size={12} />{past !== undefined ? t('gate.past', { n: past, verdict: gateText(t, gate) }) : gateText(t, gate)}
    </span>
  )
}

export function DocView({ blocks, testId }: { blocks: Block[]; testId?: string }) {
  return (
    <article data-testid={testId} className="space-y-3 text-[13.5px] leading-relaxed text-[var(--ink-2)]">
      {blocks.map((b, i) => i === 0
        ? <h2 key={i} className="serif-title text-[22px] leading-tight font-semibold text-[var(--ink)]">{b.text}</h2>
        : b.kind === 'h' ? <h3 key={i} className="pt-1 text-[14px] font-semibold text-[var(--ink)]">{b.text}</h3> : <p key={i}>{b.text}</p>)}
    </article>
  )
}

const addCls = 'rounded-sm bg-[color-mix(in_oklab,var(--ok)_22%,transparent)] text-[var(--ink)]'
const delCls = 'rounded-sm bg-[color-mix(in_oklab,var(--flag)_22%,transparent)] text-[var(--muted)] line-through'

export function DiffInline({ diff }: { diff: BlockDiff[] }) {
  return (
    <article data-testid="diff-inline" className="space-y-3 text-[13.5px] leading-relaxed text-[var(--ink-2)]">
      {diff.map((b, i) => (
        <div key={i} className={cx(b.kind === 'h' && 'font-semibold text-[var(--ink)]', b.changed && 'border-s-2 border-[var(--accent)] ps-3')}>
          {b.parts.map((p, j) => <span key={j} data-diff={p.kind} className={p.kind === 'add' ? addCls : p.kind === 'del' ? delCls : undefined}>{p.text}</span>)}
        </div>
      ))}
    </article>
  )
}

/** Side by side as a two-column table (Figma S07): a header per side, the paragraph number in the gutter, and changed
 *  rows tinted and marked − on the left and + on the right. */
export function DiffSide({ diff, left, right }: { diff: BlockDiff[]; left: string; right: string }) {
  const cell = 'grid grid-cols-[28px_minmax(0,1fr)] gap-2 px-3 py-2'
  const gutter = (n: number, mark: string, tone?: string) => (
    <span aria-hidden className={cx('pt-px text-end font-[family-name:var(--font-mono)] text-[11px] text-[var(--muted)]', tone)}>{n}{mark ? ` ${mark}` : ''}</span>
  )
  return (
    <div data-testid="diff-side" className="overflow-hidden rounded-xl border border-[var(--line)] text-[13px] leading-relaxed text-[var(--ink-2)]">
      <div className="grid grid-cols-2 border-b border-[var(--line)] bg-[var(--surface-2)]">
        <div className="caps px-3 py-2 ps-[52px]">{left}</div><div className="caps border-s border-[var(--line)] px-3 py-2 ps-[52px]">{right}</div>
      </div>
      {diff.map((b, i) => (
        <div key={i} data-changed={b.changed ? '1' : undefined} className="grid grid-cols-2 border-b border-[var(--line)]/60 last:border-b-0">
          <div className={cx(cell, b.kind === 'h' && 'font-semibold text-[var(--ink)]', b.changed && 'bg-[color-mix(in_oklab,var(--flag)_7%,transparent)]')}>
            {gutter(i + 1, b.changed ? '−' : '', b.changed ? 'text-[var(--flag)]' : undefined)}
            <div>{b.parts.filter((p) => p.kind !== 'add').map((p, j) => <span key={j} data-diff={p.kind} className={p.kind === 'del' ? delCls : undefined}>{p.text}</span>)}</div>
          </div>
          <div className={cx(cell, 'border-s border-[var(--line)]', b.kind === 'h' && 'font-semibold text-[var(--ink)]', b.changed && 'bg-[color-mix(in_oklab,var(--ok)_7%,transparent)]')}>
            {gutter(i + 1, b.changed ? '+' : '', b.changed ? 'text-[var(--ok)]' : undefined)}
            <div>{b.parts.filter((p) => p.kind !== 'del').map((p, j) => <span key={j} data-diff={p.kind} className={p.kind === 'add' ? addCls : undefined}>{p.text}</span>)}</div>
          </div>
        </div>
      ))}
    </div>
  )
}
