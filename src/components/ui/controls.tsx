// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// shadcn/ui-pattern controls on Radix primitives with the family tokens: segmented toggle, checkbox, radio cards, tabs.
import * as TG from '@radix-ui/react-toggle-group'
import * as CB from '@radix-ui/react-checkbox'
import * as RG from '@radix-ui/react-radio-group'
import * as TB from '@radix-ui/react-tabs'
import { Check } from 'lucide-react'
import type { ReactNode } from 'react'
import { cx } from '@symvoulio/ui/components'

export function Segmented<T extends string>({ value, onChange, options, label, size = 'md', testId }: {
  value: T; onChange: (v: T) => void; options: { value: T; label: ReactNode; disabled?: boolean; testId?: string; title?: string }[]; label: string; size?: 'sm' | 'md'; testId?: string
}) {
  return (
    <TG.Root type="single" value={value} aria-label={label} data-testid={testId} onValueChange={(v) => { if (v) onChange(v as T) }}
      className="inline-flex shrink-0 rounded-lg border border-[var(--line)] bg-[var(--surface-2)] p-0.5">
      {options.map((o) => (
        <TG.Item key={o.value} value={o.value} disabled={o.disabled} data-testid={o.testId} title={o.title}
          className={cx('rounded-md font-medium whitespace-nowrap text-[var(--muted)] transition-colors duration-150 hover:text-[var(--ink)] disabled:cursor-not-allowed disabled:opacity-40 data-[state=on]:bg-[var(--accent)] data-[state=on]:text-[var(--on-accent)] data-[state=on]:shadow-[var(--shadow-card)] data-[state=on]:hover:text-[var(--on-accent)] focus-visible:outline-2 focus-visible:outline-[var(--accent)]',
            size === 'sm' ? 'px-2 py-0.5 text-[11px]' : 'px-3 py-1.5 text-[13px]')}>
          {o.label}
        </TG.Item>
      ))}
    </TG.Root>
  )
}

export function Checkbox({ checked, onChange, label, hint, disabled, testId }: { checked: boolean; onChange?: (v: boolean) => void; label: ReactNode; hint?: ReactNode; disabled?: boolean; testId?: string }) {
  return (
    <label className={cx('flex items-start gap-2.5 py-1 text-[13px]', disabled ? 'cursor-not-allowed' : 'cursor-pointer')}>
      <CB.Root checked={checked} disabled={disabled} data-testid={testId} onCheckedChange={(v) => onChange?.(v === true)}
        className="mt-0.5 grid size-[18px] shrink-0 place-items-center rounded-[5px] border border-[var(--line)] bg-[var(--surface)] text-[var(--on-accent)] data-[state=checked]:border-[var(--accent)] data-[state=checked]:bg-[var(--accent)] disabled:opacity-60 focus-visible:outline-2 focus-visible:outline-[var(--accent)]">
        <CB.Indicator><Check size={13} strokeWidth={3} /></CB.Indicator>
      </CB.Root>
      <span><span className="text-[var(--ink)]">{label}</span>{hint ? <span className="block text-[12px] text-[var(--muted)]">{hint}</span> : null}</span>
    </label>
  )
}

export function RadioCards<T extends string>({ value, onChange, options, label }: {
  value: T; onChange: (v: T) => void; options: { value: T; title: ReactNode; text: ReactNode; testId?: string }[]; label: string
}) {
  return (
    <RG.Root value={value} onValueChange={(v) => onChange(v as T)} aria-label={label} className="grid grid-cols-3 gap-3 max-[1500px]:grid-cols-1">
      {options.map((o) => (
        <RG.Item key={o.value} value={o.value} data-testid={o.testId}
          className="group flex flex-col items-stretch justify-start rounded-xl border border-[var(--line)] bg-[var(--surface)] p-3 text-start transition-[border,box-shadow] duration-150 hover:border-[var(--accent)] data-[state=checked]:border-[var(--accent)] data-[state=checked]:shadow-[0_0_0_1px_var(--accent)] focus-visible:outline-2 focus-visible:outline-[var(--accent)]">
          <span className="flex items-center gap-2 text-[13.5px] font-semibold text-[var(--ink)]">
            <span className="grid size-4 place-items-center rounded-full border border-[var(--line)] group-data-[state=checked]:border-[var(--accent)]">
              <RG.Indicator className="size-2 rounded-full bg-[var(--accent)]" />
            </span>
            {o.title}
          </span>
          <span className="mt-1 block text-[12px] leading-snug text-[var(--muted)]">{o.text}</span>
        </RG.Item>
      ))}
    </RG.Root>
  )
}

export const Tabs = TB.Root
export const TabsContent = TB.Content
export function TabsList({ children, label, className }: { children: ReactNode; label: string; className?: string }) {
  return <TB.List aria-label={label} className={cx('flex gap-1 border-b border-[var(--line)]', className)}>{children}</TB.List>
}
export function TabsTrigger({ value, children, testId, disabled }: { value: string; children: ReactNode; testId?: string; disabled?: boolean }) {
  return (
    <TB.Trigger value={value} data-testid={testId} disabled={disabled}
      className="-mb-px border-b-2 border-transparent px-3 py-2 text-[13px] font-medium whitespace-nowrap text-[var(--muted)] hover:text-[var(--ink)] disabled:opacity-40 data-[state=active]:border-[var(--accent)] data-[state=active]:text-[var(--ink)] focus-visible:outline-2 focus-visible:outline-[var(--accent)]">
      {children}
    </TB.Trigger>
  )
}

export function Field({ label, children, hint, htmlFor }: { label: ReactNode; children: ReactNode; hint?: ReactNode; htmlFor?: string }) {
  return (
    <div className="space-y-1.5">
      <label htmlFor={htmlFor} className="caps block">{label}</label>
      {children}
      {hint ? <p className="text-[12px] text-[var(--muted)]">{hint}</p> : null}
    </div>
  )
}
export const inputCls = 'w-full rounded-lg border border-[var(--line)] bg-[var(--surface-2)] px-3 py-2 text-[13.5px] text-[var(--ink)] placeholder:text-[var(--muted)] outline-none transition-colors focus:border-[var(--accent)]'
