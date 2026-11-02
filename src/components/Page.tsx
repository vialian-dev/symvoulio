// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// A screen inside the frame: caption in wide-tracked caps, serif small-caps title with an accent word, actions at the end
// (the family ScreenPage idiom), entering with a short glide. A screen with a banner place shows it above the title.
import type { ReactNode } from 'react'
import { motion } from 'motion/react'
import { Caps, cx } from '@symvoulio/ui/components'
import { EASE_GLIDE } from '@symvoulio/ui/tokens'
import { useTranslation } from 'react-i18next'
import { Check } from 'lucide-react'
import { PageBanner } from '../frame/Banner'

export function Page({ id, caption, title, accent, actions, children, className, banner }: {
  id: string; banner?: boolean; caption: ReactNode; title: string; accent?: string; actions?: ReactNode; children: ReactNode; className?: string
}) {
  return (
    <motion.section data-screen={id} initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.35, ease: EASE_GLIDE }}
      className={cx('mx-auto max-w-[1180px] px-7 pt-6 pb-10 max-[1440px]:px-5', className)}>
      {banner && <PageBanner screen={id} />}
      <div className="mb-5 flex flex-wrap items-end justify-between gap-4">
        <div className="min-w-0">
          <Caps>{caption}</Caps>
          <h1 data-testid="screen-title" className="serif-title mt-1 text-[34px] leading-none font-semibold">
            {title}{accent ? <> <span className="text-[var(--accent)]">{accent}</span></> : null}
          </h1>
        </div>
        {actions ? <div className="flex flex-wrap items-center gap-2">{actions}</div> : null}
      </div>
      {children}
    </motion.section>
  )
}

const STEPS = ['system', 'claude', 'codex', 'roles', 'policy'] as const
/** The wizard's five steps (S1–S5) with the current one marked. */
export function Stepper({ current }: { current: (typeof STEPS)[number] }) {
  const { t } = useTranslation()
  const at = STEPS.indexOf(current)
  return (
    <ol data-testid="stepper" aria-label={t('setup.steps')} className="mb-6 flex flex-wrap items-center gap-2">
      {STEPS.map((s, i) => (
        <li key={s} className="flex items-center gap-2" aria-current={i === at ? 'step' : undefined}>
          <span className={cx('grid size-6 place-items-center rounded-full text-[11px] font-semibold',
            i < at ? 'bg-[var(--accent)] text-[var(--on-accent)]' : i === at ? 'border-2 border-[var(--accent)] text-[var(--accent)]' : 'border border-[var(--line)] text-[var(--muted)]')}>
            {i < at ? <Check size={12} strokeWidth={3} /> : i + 1}
          </span>
          <span className={cx('text-[12.5px]', i === at ? 'font-medium text-[var(--ink)]' : 'text-[var(--muted)]')}>{t(`setup.step.${s}`)}</span>
          {i < STEPS.length - 1 && <span className="mx-1 h-px w-6 bg-[var(--line)]" />}
        </li>
      ))}
    </ol>
  )
}
