// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Agent panel shell (side panel under the top bar, non-modal) and the change preview dialog «Before / After» with its
// journal. The product supplies the scenarios, the log texts and what «Yes» does.
import { useState, type ReactNode } from 'react'
import * as Dialog from '@radix-ui/react-dialog'
import { Undo2, X } from 'lucide-react'
import { Button, Caps } from '../components/ui'
import { cx } from '../cx'

export interface AgentLogLine { who: 'me' | 'agent'; text: ReactNode }
export interface AgentAction { id: string; label: ReactNode; onClick: () => void; edge?: string }

export function AgentPanel({ open, onOpenChange, title, description, closeLabel, tryLabel, actions, log, placeholder, sendLabel, onSend }: {
  open: boolean
  onOpenChange: (o: boolean) => void
  title: ReactNode
  /** Caption under the title (whose context, which rights). */
  description: ReactNode
  closeLabel: string
  /** Caption above the suggested actions («Try»). */
  tryLabel: ReactNode
  /** Suggested actions as chips; each carries data-testid = id. */
  actions: AgentAction[]
  log: AgentLogLine[]
  placeholder: string
  sendLabel: ReactNode
  /** A typed message (trimmed, not empty); the input clears after it. */
  onSend: (text: string) => void
}) {
  const [text, setText] = useState('')
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange} modal={false}>
      <Dialog.Portal>
        <Dialog.Content data-testid="A1" onInteractOutside={(e) => e.preventDefault()}
          className="fixed top-[60px] end-0 bottom-0 z-40 flex w-[400px] flex-col border-s border-[var(--line)] bg-[var(--surface)] shadow-[var(--shadow-lift)]">
          <div className="flex items-center justify-between border-b border-[var(--line)] px-4 py-3">
            <div>
              <Dialog.Title className="serif-title text-[20px] font-semibold">{title}</Dialog.Title>
              <Dialog.Description className="caps mt-0.5">{description}</Dialog.Description>
            </div>
            <Dialog.Close aria-label={closeLabel} className="rounded p-1 hover:bg-[var(--surface-2)]"><X size={16} aria-hidden /></Dialog.Close>
          </div>
          <div className="flex-1 space-y-3 overflow-y-auto px-4 py-4">
            <Caps>{tryLabel}</Caps>
            <div className="flex flex-wrap gap-2">
              {actions.map((a) => (
                <Button key={a.id} variant="ghost" data-testid={a.id} data-edge={a.edge} onClick={a.onClick}>{a.label}</Button>
              ))}
            </div>
            <div role="log" aria-live="polite" className="space-y-3" data-testid="a1-log">
              {log.map((m, i) => (
                <div key={i} className={cx('max-w-[88%] rounded-xl px-3 py-2 text-[13px] whitespace-pre-line', m.who === 'me' ? 'ms-auto bg-[var(--accent)] text-[var(--on-accent)]' : 'bg-[var(--surface-2)]')}>{m.text}</div>
              ))}
            </div>
          </div>
          <form className="flex gap-2 border-t border-[var(--line)] p-3" onSubmit={(e) => { e.preventDefault(); if (text.trim()) { onSend(text); setText('') } }}>
            <input value={text} onChange={(e) => setText(e.target.value)} placeholder={placeholder} aria-label={placeholder}
              className="flex-1 rounded-lg border border-[var(--line)] bg-[var(--surface)] px-3 py-2 text-[13px] outline-none focus:border-[var(--accent)]" />
            <Button variant="primary" type="submit">{sendLabel}</Button>
          </form>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  )
}

export interface JournalRow { id: number | string; title: ReactNode; before: ReactNode; after: ReactNode; onRollback?: () => void }

/** Change preview: what changes, Before and After side by side, No / Yes, and the last three journal rows with rollback. */
export function PreviewDialog({ open, onClose, caption, title, before, after, beforeLabel, afterLabel, note, noLabel, yesLabel, onYes, journalLabel, journal, rollbackLabel }: {
  open: boolean
  onClose: () => void
  caption: string
  title: ReactNode
  before: ReactNode
  after: ReactNode
  beforeLabel: ReactNode
  afterLabel: ReactNode
  note: ReactNode
  noLabel: ReactNode
  yesLabel: ReactNode
  onYes: () => void
  journalLabel: ReactNode
  journal: JournalRow[]
  rollbackLabel: ReactNode
}) {
  return (
    <Dialog.Root open={open} onOpenChange={(o) => { if (!o) onClose() }}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-50 bg-[var(--ink)]/20 backdrop-blur-[2px]" />
        <Dialog.Content data-testid="A2" className="fixed inset-0 z-50 m-auto h-fit w-[520px] rounded-[var(--radius-card)] border border-[var(--line)] bg-[var(--surface)] p-6 shadow-[var(--shadow-lift)]">
          {open && (
            <>
              <Caps>{caption}</Caps>
              <Dialog.Title className="serif-title mt-1 text-[24px] font-semibold">{title}</Dialog.Title>
              <Dialog.Description className="sr-only">{caption}</Dialog.Description>
              <BeforeAfter before={before} after={after} labels={{ before: beforeLabel, after: afterLabel }} />
              <p className="mt-3 text-[12px] text-[var(--muted)]">{note}</p>
              <div className="mt-5 flex justify-end gap-2">
                <Button data-testid="a2-no" onClick={onClose}>{noLabel}</Button>
                <Button data-testid="a2-yes" variant="primary" onClick={onYes}>{yesLabel}</Button>
              </div>
              {journal.length > 0 && (
                <div className="mt-5 border-t border-[var(--line)] pt-3">
                  <Caps>{journalLabel}</Caps>
                  {journal.slice(0, 3).map((j) => (
                    <div key={j.id} className="mt-1 flex items-center justify-between text-[12px]">
                      <span>{j.title}: <bdi>{j.before}</bdi> <span aria-hidden>→</span> <bdi>{j.after}</bdi></span>
                      {j.onRollback && <button type="button" data-testid="a2-rollback" className="flex items-center gap-1 text-[var(--accent)]" onClick={j.onRollback}><Undo2 size={12} aria-hidden />{rollbackLabel}</button>}
                    </div>
                  ))}
                </div>
              )}
            </>
          )}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  )
}

/** Before / After side by side: the old value plain, the new one in the accent box. */
export function BeforeAfter({ before, after, labels }: { before: ReactNode; after: ReactNode; labels: { before: ReactNode; after: ReactNode } }) {
  return (
    <div className="mt-4 grid grid-cols-2 gap-3">
      <div className="rounded-xl border border-[var(--line)] p-3"><Caps>{labels.before}</Caps><div className="mt-1 text-[15px]" data-testid="a2-before">{before}</div></div>
      <div className="rounded-xl border border-[var(--accent)] bg-[var(--accent-soft)] p-3"><Caps>{labels.after}</Caps><div className="mt-1 text-[15px] text-[var(--accent)]" data-testid="a2-after">{after}</div></div>
    </div>
  )
}
