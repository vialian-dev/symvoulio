// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Charts of the design system (added for S21 / S22, 30.10): a line chart over time and a heatmap, drawn in the existing
// visual language — theme tokens only (accent, value, flag, line, muted), no new styles. Both are plain SVG / CSS grid,
// mirror in RTL by layout, and carry data-* attributes so a test can read the values they draw.
import type { ReactNode } from 'react'
import { cx } from '../cx'

export interface Series { id: string; label: string; points: number[]; tone?: 'accent' | 'value' | 'flag' | 'muted' }
const STROKE = { accent: 'var(--accent)', value: 'var(--value)', flag: 'var(--flag)', muted: 'var(--muted)' } as const

/** Line chart: one line per series over the same buckets; x labels under the first, middle and last bucket. */
export function LineChart({ series, labels, height = 180, label, format = (n) => String(n) }: {
  series: Series[]; labels: string[]; height?: number; label: string; format?: (n: number) => string
}) {
  const W = 600, H = height, P = 28
  const n = Math.max(1, labels.length - 1)
  const max = Math.max(1, ...series.flatMap((s) => s.points))
  const x = (i: number) => P + ((W - 2 * P) * i) / n
  const y = (v: number) => H - P - ((H - 2 * P) * v) / max
  return (
    <figure className="m-0" data-chart="line">
      <svg viewBox={`0 0 ${W} ${H}`} className="block w-full rtl:-scale-x-100" role="img" aria-label={label}>
        {[0, 0.5, 1].map((f) => <line key={f} x1={P} x2={W - P} y1={y(max * f)} y2={y(max * f)} stroke="var(--line)" strokeDasharray={f ? '3 4' : undefined} />)}
        {series.map((s) => (
          <polyline key={s.id} data-series={s.id} data-sum={s.points.reduce((a, b) => a + b, 0)} fill="none" stroke={STROKE[s.tone ?? 'accent']} strokeWidth="2"
            strokeLinejoin="round" strokeLinecap="round" points={s.points.map((v, i) => `${x(i)},${y(v)}`).join(' ')} />
        ))}
      </svg>
      <figcaption className="mt-1 flex justify-between text-[11px] text-[var(--muted)]">
        <span>{labels[0]}</span><span>{labels[Math.floor(labels.length / 2)]}</span><span>{labels[labels.length - 1]}</span>
      </figcaption>
      <div className="mt-2 flex flex-wrap gap-3 text-[11.5px] text-[var(--ink-2)]">
        {series.map((s) => (
          <span key={s.id} className="inline-flex items-center gap-1.5">
            <span className="h-0.5 w-4 rounded" style={{ background: STROKE[s.tone ?? 'accent'] }} />{s.label}
            <span className="num text-[12.5px] text-[var(--muted)]">{format(s.points.reduce((a, b) => a + b, 0))}</span>
          </span>
        ))}
      </div>
    </figure>
  )
}

/** Heatmap: rows × columns of values, cell intensity by the accent token; row labels may be nodes (links). */
export function Heatmap({ rows, cols, values, label, cellTitle }: {
  rows: { id: string; label: ReactNode }[]; cols: string[]; values: number[][]; label: string; cellTitle?: (r: number, c: number) => string
}) {
  const max = Math.max(1, ...values.flat())
  return (
    <div role="table" aria-label={label} className="overflow-x-auto" data-chart="heatmap">
      <div className="grid min-w-[560px] gap-px text-[11px]" style={{ gridTemplateColumns: `minmax(120px, 160px) repeat(${cols.length}, minmax(14px, 1fr))` }}>
        <span />
        {cols.map((c, i) => <span key={i} role="columnheader" className="num text-center text-[10px] text-[var(--muted)]">{i % 3 === 0 ? c : ''}</span>)}
        {rows.map((r, ri) => [
          <span key={`${r.id}-h`} role="rowheader" className="truncate pe-2 text-[var(--ink-2)]">{r.label}</span>,
          ...cols.map((_, ci) => {
            const v = values[ri]?.[ci] ?? 0
            return (
              <span key={`${r.id}-${ci}`} role="cell" data-row={r.id} data-col={ci} data-v={v} title={cellTitle?.(ri, ci)}
                className={cx('h-5 rounded-[3px]', v === 0 && 'bg-[var(--surface-2)]')}
                style={v ? { background: `color-mix(in oklab, var(--accent) ${Math.round(15 + (85 * v) / max)}%, var(--surface))` } : undefined} />
            )
          }),
        ])}
      </div>
    </div>
  )
}
