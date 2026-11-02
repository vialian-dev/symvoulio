// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// «All screens» page pattern: every screen of the product as a card, with where the viewer finds it — in the menu, by a
// link from another screen, or not at all (no right). Open to every role: the left menu shows only the role's sections,
// this page shows every screen and says which the role may open.
import type { ReactNode } from 'react'
import { Link } from 'react-router'
import { ArrowRight, Lock } from 'lucide-react'
import { Badge, Caps, Card } from '../components/ui'

export type ScreenWhere = 'inMenu' | 'byLink' | 'noRight'
export interface ScreenCard {
  id: string
  to: string
  title: ReactNode
  caption: ReactNode
  where: ScreenWhere
  /** Text of the `where` badge. */
  whereLabel: ReactNode
}

export function AllScreensPage({ caption, title, intro, screens, openLabel }: {
  caption: ReactNode; title: ReactNode; intro: ReactNode; screens: ScreenCard[]; openLabel: ReactNode
}) {
  return (
    <section data-screen="ALL" data-testid="all-screens">
      <div className="mb-5">
        <Caps>{caption}</Caps>
        <h1 className="serif-title mt-1 text-[36px] leading-none font-semibold">{title}</h1>
        <p className="mt-2 max-w-[760px] text-[13.5px] text-[var(--ink-2)]">{intro}</p>
      </div>
      <ul className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
        {screens.map((s) => (
          <li key={s.id}>
            <Card className="h-full p-0">
              <Link to={s.to} data-testid={`all-screens-link-${s.id}`} data-target={s.id}
                className="group flex h-full flex-col gap-2 rounded-[inherit] p-4 !no-underline hover:bg-[var(--surface-2)]">
                <div className="flex items-center justify-between gap-2">
                  <span className="num text-[12px] tracking-[0.12em] text-[var(--muted)]"><bdi>{s.id}</bdi></span>
                  <span data-testid={`all-screens-where-${s.id}`} data-where={s.where}><Badge tone={s.where === 'noRight' ? 'plain' : s.where === 'inMenu' ? 'accent' : 'value'}>
                    {s.where === 'noRight' && <Lock size={11} aria-hidden />}{s.whereLabel}
                  </Badge></span>
                </div>
                <div className="serif-title text-[20px] font-semibold text-[var(--ink)]">{s.title}</div>
                <div className="text-[12.5px] text-[var(--muted)]">{s.caption}</div>
                <span className="mt-auto inline-flex items-center gap-1 pt-1 text-[12.5px] text-[var(--accent)]">
                  {openLabel}<ArrowRight size={12} aria-hidden className="rtl:rotate-180" />
                </span>
              </Link>
            </Card>
          </li>
        ))}
      </ul>
    </section>
  )
}
