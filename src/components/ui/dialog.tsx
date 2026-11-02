// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// shadcn/ui-pattern wrappers on Radix Dialog, styled with the family tokens: a centred dialog and a side sheet (drawer).
import * as D from '@radix-ui/react-dialog'
import { X } from 'lucide-react'
import type { ReactNode } from 'react'
import { cx } from '@symvoulio/ui/components'

export const Dialog = D.Root
export const DialogClose = D.Close

export function DialogContent({ title, description, children, closeLabel, className, testId }: {
  title: ReactNode; description?: ReactNode; children: ReactNode; closeLabel: string; className?: string; testId?: string
}) {
  return (
    <D.Portal>
      <D.Overlay className="fixed inset-0 z-40 bg-[#020817]/70 backdrop-blur-[2px]" />
      <D.Content data-testid={testId} className={cx('fixed top-1/2 left-1/2 z-50 w-[min(520px,92vw)] -translate-x-1/2 -translate-y-1/2 rounded-[var(--radius-card)] border border-[var(--line)] bg-[var(--surface)] p-6 shadow-[var(--shadow-lift)] focus:outline-none', className)}>
        <D.Title className="serif-title pe-8 text-[24px] font-semibold leading-tight">{title}</D.Title>
        {description ? <D.Description className="mt-1 text-[13px] text-[var(--muted)]">{description}</D.Description> : <D.Description className="sr-only">{title}</D.Description>}
        <div className="mt-4">{children}</div>
        <D.Close aria-label={closeLabel} className="absolute top-4 end-4 rounded p-1 text-[var(--muted)] hover:bg-[var(--surface-2)] hover:text-[var(--ink)]"><X size={16} /></D.Close>
      </D.Content>
    </D.Portal>
  )
}

export function SheetContent({ title, children, closeLabel, testId }: { title: ReactNode; children: ReactNode; closeLabel: string; testId?: string }) {
  return (
    <D.Portal>
      <D.Overlay className="fixed inset-0 z-40 bg-[#020817]/55" />
      <D.Content data-testid={testId} className="fixed top-0 end-0 bottom-0 z-50 flex w-[min(560px,92vw)] flex-col border-s border-[var(--line)] bg-[var(--surface)] shadow-[var(--shadow-lift)] focus:outline-none">
        <div className="flex items-start justify-between gap-3 border-b border-[var(--line)] px-6 py-4">
          <D.Title className="serif-title text-[24px] font-semibold leading-tight">{title}</D.Title>
          <D.Description className="sr-only">{title}</D.Description>
          <D.Close aria-label={closeLabel} data-testid="sheet-close" className="rounded p-1 text-[var(--muted)] hover:bg-[var(--surface-2)] hover:text-[var(--ink)]"><X size={18} /></D.Close>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">{children}</div>
      </D.Content>
    </D.Portal>
  )
}
