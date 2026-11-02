// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Virtualised table with fixed row height: only visible rows are in the DOM (gate «10 000 rows without jank»).
// ARIA table semantics for a virtual list: table > rowgroup > row > cell, with aria-rowcount / aria-rowindex so assistive
// technology knows the full size and the position of every rendered row. the shared UI kit uses it on Finance (S14) and Setup (S17).
import { memo, useRef, type CSSProperties, type ReactNode } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import { cx } from '../cx'

export function VirtualRows<T>({ rows, cols, head, render, label, rowHeight = 40, height = 440, testId = 'big-list' }: {
  rows: T[]; cols: string; head: ReactNode[]; render: (row: T, index: number) => ReactNode[]; label: string; rowHeight?: number; height?: number; testId?: string
}) {
  const ref = useRef<HTMLDivElement>(null)
  const v = useVirtualizer({ count: rows.length, getScrollElement: () => ref.current, estimateSize: () => rowHeight, overscan: 8 })
  // Below 1024 px the table keeps a minimum width (fixed columns at their size, 120 px per fr) and scrolls inside its own box,
  // so column headers never squeeze to one letter per line. Wide layouts are unchanged.
  const minWidth = cols.split(/\s+/).reduce((s, c) => s + (c.endsWith('px') ? parseFloat(c) : c.endsWith('fr') ? 120 * parseFloat(c) : 100), 0)
  return (
    <div className="max-lg:overflow-x-auto">
    <div role="table" aria-label={label} aria-rowcount={rows.length + 1} className="text-[13px] max-lg:min-w-[var(--vr-min)]" style={{ '--vr-min': `${minWidth}px` } as CSSProperties}>
      <div role="rowgroup">
        <div role="row" aria-rowindex={1} className="grid border-b border-[var(--line)] px-1" style={{ gridTemplateColumns: cols }}>
          {head.map((h, i) => <div key={i} role="columnheader" className="caps px-2 py-2 text-start">{h}</div>)}
        </div>
      </div>
      <div ref={ref} role="rowgroup" tabIndex={0} aria-label={label} data-testid={testId} data-rows={rows.length}
        className="overflow-y-auto overscroll-contain px-1 focus-visible:outline-2 focus-visible:outline-[var(--accent)]" style={{ height, contain: 'strict' }}>
        <div className="relative w-full" style={{ height: v.getTotalSize() }}>
          {v.getVirtualItems().map((it) => (
            <Row key={it.key} index={it.index} start={it.start} height={rowHeight} cols={cols} row={rows[it.index]} render={render as Render} />
          ))}
        </div>
      </div>
    </div>
    </div>
  )
}

type Render = (row: unknown, index: number) => ReactNode[]
/**
 * One virtual row, memoised: while scrolling only rows that enter the window render; rows that stay only move (transform).
 * Each frame used to re-render every visible row with its tooltips and links (scroll gate 44–48 fps instead of > 50).
 */
const Row = memo(function Row({ index, start, height, cols, row, render }: { index: number; start: number; height: number; cols: string; row: unknown; render: Render }) {
  return (
    <div data-index={index} data-row="" role="row" aria-rowindex={index + 2}
      className={cx('absolute start-0 end-0 top-0 grid items-center border-b border-[var(--line)]/60 hover:bg-[var(--surface-2)]/60', index % 2 === 1 && 'bg-[var(--surface-2)]/25')}
      style={{ gridTemplateColumns: cols, height, transform: `translateY(${start}px)` }}>
      {render(row, index).map((c, i) => <div key={i} role="cell" className="min-w-0 truncate px-2">{c}</div>)}
    </div>
  )
})
