// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Base components of the shared UI kit: white rounded cards with a soft shadow,
// wide-tracked caps between thin rules, serif small-caps titles, cobalt check, numbers that explain themselves.
// No product data and no locale keys here: every text comes in by props.
import { forwardRef, useEffect, useRef, useState, type ButtonHTMLAttributes, type ReactNode } from 'react'
import * as Tooltip from '@radix-ui/react-tooltip'
import { Check } from 'lucide-react'
import { cx } from '../cx'
import { fmt } from '../i18n'

export { cx }

export function Card({ children, className, ...rest }: { children: ReactNode; className?: string } & React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div {...rest} className={cx('rounded-[var(--radius-card)] border border-[var(--line)] bg-[var(--surface)] shadow-[var(--shadow-card)] max-lg:min-w-0 max-lg:overflow-x-auto', className)}>
      {children}
    </div>
  )
}

/** Subtitle style of the banners: wide-tracked caps between two thin rules. */
export function Caps({ children, rules = false, className }: { children: ReactNode; rules?: boolean; className?: string }) {
  if (!rules) return <div className={cx('caps', className)}>{children}</div>
  return (
    <div className={cx('flex items-center gap-3', className)}>
      <span className="h-px flex-1 bg-[var(--line)]" />
      <span className="caps whitespace-nowrap">{children}</span>
      <span className="h-px flex-1 bg-[var(--line)]" />
    </div>
  )
}

export function Title({ children, accent, className }: { children: ReactNode; accent?: ReactNode; className?: string }) {
  return (
    <h1 className={cx('serif-title text-[34px] leading-tight font-semibold', className)}>
      {children}{accent ? <> <span className="text-[var(--accent)]">{accent}</span></> : null}
    </h1>
  )
}

type BtnProps = ButtonHTMLAttributes<HTMLButtonElement> & { variant?: 'primary' | 'ghost' | 'quiet' | 'danger' }
export const Button = forwardRef<HTMLButtonElement, BtnProps>(function Button({ variant = 'ghost', className, ...rest }, ref) {
  return (
    <button
      ref={ref}
      {...rest}
      className={cx(
        'inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-[13px] font-medium focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)] transition-[background,box-shadow,transform] duration-150 ease-[var(--ease-glide)] active:scale-[0.98] disabled:opacity-40',
        variant === 'primary' && 'bg-[var(--accent)] text-[var(--on-accent)] shadow-[var(--shadow-card)] hover:brightness-110',
        variant === 'ghost' && 'border border-[var(--line)] bg-[var(--surface)] text-[var(--ink-2)] hover:border-[var(--accent)] hover:text-[var(--accent)]',
        variant === 'quiet' && 'text-[var(--ink-2)] hover:bg-[var(--surface-2)]',
        variant === 'danger' && 'border border-[var(--flag)] text-[var(--flag)] hover:bg-[var(--flag-soft)]',
        className,
      )}
    />
  )
})

export function Badge({ children, tone = 'plain', className }: { children: ReactNode; tone?: 'plain' | 'accent' | 'value' | 'flag' | 'ok'; className?: string }) {
  return (
    <span className={cx('inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-medium whitespace-nowrap',
      tone === 'plain' && 'bg-[var(--surface-2)] text-[var(--ink-2)]',
      tone === 'accent' && 'bg-[var(--accent-soft)] text-[var(--accent)]',
      tone === 'value' && 'bg-[var(--value-soft)] text-[color-mix(in_oklab,var(--value)_70%,var(--ink))]',
      tone === 'flag' && 'bg-[var(--flag-soft)] text-[var(--flag)]',
      tone === 'ok' && 'bg-[var(--accent-soft)] text-[var(--accent)]', className)}>
      {children}
    </span>
  )
}

/** The banner check badge: a human Approval. */
export function ApprovalCheck({ title }: { title?: string }) {
  return (
    <span title={title} role={title ? 'img' : undefined} aria-label={title} aria-hidden={title ? undefined : true} className="inline-grid size-4 place-items-center rounded-full bg-[var(--accent)] text-[var(--on-accent)]">
      <Check size={11} strokeWidth={3} />
    </span>
  )
}

/**
 * Every number explains itself: formula and source on hover AND on keyboard focus — the trigger is focusable
 * (tabIndex 0) and Radix links the tooltip by aria-describedby, so keyboard and screen-reader users get the same explanation.
 * `why` is resolved at render time (follows the language).
 */
export function Explain({ children, why, edge }: { children: ReactNode; why: string; edge?: string }) {
  // The tooltip machinery mounts on first hover / focus only: long virtual lists render thousands of numbers, and a Radix
  // tooltip per cell cost the 10 000-row scroll gate several fps. Same element, same focus, same explanation.
  const [armed, setArmed] = useState<'hover' | 'focus' | null>(null)
  const ref = useRef<HTMLSpanElement>(null)
  const hoverTimer = useRef<number | undefined>(undefined)
  useEffect(() => { if (armed === 'focus') ref.current?.focus() }, [armed])
  useEffect(() => () => window.clearTimeout(hoverTimer.current), [])
  const cls = 'cursor-help rounded-sm underline decoration-dotted decoration-[var(--mist-400,#8aa0bd)] underline-offset-4 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]'
  if (!armed) {
    // Hover arms after a short rest (as the tooltip's own delay) — arming re-mounts the span, which must not happen between
    // pointer-down and click of a link that contains the number.
    return <span ref={ref} tabIndex={0} data-explain={edge ?? ''} className={cls}
      onPointerEnter={() => { hoverTimer.current = window.setTimeout(() => setArmed('hover'), 150) }}
      onPointerLeave={() => window.clearTimeout(hoverTimer.current)} onPointerDown={() => window.clearTimeout(hoverTimer.current)}
      onFocus={(e) => { if (e.currentTarget.matches(':focus-visible')) setArmed('focus') }}>{children}</span>
  }
  return (
    <Tooltip.Root delayDuration={120} defaultOpen>
      <Tooltip.Trigger asChild>
        <span ref={ref} tabIndex={0} data-explain={edge ?? ''} className={cls}>{children}</span>
      </Tooltip.Trigger>
      <Tooltip.Portal>
        <Tooltip.Content sideOffset={6} className="z-50 max-w-xs rounded-lg border border-[var(--line)] bg-[var(--surface)] px-3 py-2 text-[12px] text-[var(--ink-2)] shadow-[var(--shadow-lift)]">
          {why}
          <Tooltip.Arrow className="fill-[var(--surface)]" />
        </Tooltip.Content>
      </Tooltip.Portal>
    </Tooltip.Root>
  )
}

export function Kpi({ label, value, why, tone, edge }: { label: string; value: ReactNode; why: string; tone?: 'value' | 'flag'; edge?: ReactNode }) {
  return (
    <Card className="px-5 py-4">
      <Caps>{label}</Caps>
      <div className={cx('num mt-1 text-[30px] font-semibold leading-none', tone === 'value' && 'text-[var(--value-ink)]', tone === 'flag' && 'text-[var(--flag)]')}>
        <Explain why={why}>{value}</Explain>
      </div>
      {edge ? <div className="mt-2 text-[12px]">{edge}</div> : null}
    </Card>
  )
}

export function Table({ head, children, className }: { head: ReactNode[]; children: ReactNode; className?: string }) {
  return (
    <div className={cx('overflow-x-auto', className)}>
      <table className="w-full border-collapse text-[13px]">
        <thead>
          <tr className="border-b border-[var(--line)] text-start">
            {head.map((h, i) => <th key={i} className="caps px-3 py-2 font-medium">{h}</th>)}
          </tr>
        </thead>
        <tbody className="[&>tr]:border-b [&>tr]:border-[var(--line)]/60 [&>tr:hover]:bg-[var(--surface-2)]/60 [&_td]:px-3 [&_td]:py-2">
          {children}
        </tbody>
      </table>
    </div>
  )
}

/**
 * Money with its explanation: the amount in the current language's currency format, dotted underline, formula and source
 * on hover and keyboard focus (Explain). `hidden` is what a viewer without the money right sees instead.
 */
export function Money({ value, why, hidden }: { value: number; why: string; hidden?: ReactNode }) {
  if (hidden !== undefined) return <>{hidden}</>
  return <Explain why={why}>{fmt.money(value)}</Explain>
}

/** Empty list or nothing-to-show line, in the muted text of the screens. `card` puts it in a Card as S09/S11/S12 do. */
export function EmptyState({ children, card, className }: { children: ReactNode; card?: boolean; className?: string }) {
  if (card) return <Card className={cx('p-6 text-[13px] text-[var(--muted)]', className)}>{children}</Card>
  return <p className={cx('text-[13px] text-[var(--muted)]', className)}>{children}</p>
}
