// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// A screen inside the frame: optional banner, caption in wide-tracked caps, serif small-caps title with an accent word,
// read-only badge, actions on the end side. And the page a role may not open.
import type { ReactNode } from 'react'
import { Badge, Caps } from '../components/ui'

export function ScreenPage({ id, caption, title, accent, titleFull, readOnlyLabel, banner, actions, children }: {
  /** data-screen of the section. */
  id: string
  caption: ReactNode
  title: string
  /** A word of the title in the accent colour. */
  accent?: string
  /** A whole translated title that contains `accent`: word order stays the language's own, not glued from two keys. */
  titleFull?: string
  /** Set when the viewer may only read this screen. */
  readOnlyLabel?: ReactNode
  /** Usually <PageBanner screen={id} />. */
  banner?: ReactNode
  actions?: ReactNode
  children: ReactNode
}) {
  return (
    <section data-screen={id}>
      {banner}
      <div className="mb-5 flex items-end justify-between gap-4">
        <div>
          <Caps>{caption}</Caps>
          <h1 className="serif-title mt-1 text-[36px] leading-none font-semibold">
            {titleFull && accent && titleFull.includes(accent)
              ? <>{titleFull.slice(0, titleFull.indexOf(accent))}<span className="text-[var(--accent)]">{accent}</span>{titleFull.slice(titleFull.indexOf(accent) + accent.length)}</>
              : <>{title}{accent ? <> <span className="text-[var(--accent)]">{accent}</span></> : null}</>}
          </h1>
          {readOnlyLabel ? <Badge className="mt-2">{readOnlyLabel}</Badge> : null}
        </div>
        <div className="flex gap-2">{actions}</div>
      </div>
      {children}
    </section>
  )
}

/** A screen the viewer has no right to: no entity name anywhere, a caption, a title and one line of text. */
export function DeniedPage({ id, caption, title, text }: { id: string; caption: ReactNode; title: ReactNode; text: ReactNode }) {
  return (
    <div data-screen={id} data-denied="1" className="mt-16 text-center">
      <Caps rules>{caption}</Caps>
      <h1 className="serif-title mt-3 text-[30px]" data-testid="denied">{title}</h1>
      <p className="mt-2 text-[var(--muted)]">{text}</p>
    </div>
  )
}
