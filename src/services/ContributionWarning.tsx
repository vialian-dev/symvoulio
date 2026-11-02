// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { useTranslation } from 'react-i18next'
import { useRunData } from './useRunData'
export function ContributionWarning({ id, round }: { id: string; round?: number }) {
 const { t } = useTranslation()
 const { snapshot } = useRunData(id)
 return <>{(snapshot?.rounds ?? []).filter(r => r.contributionExceeded && (round === undefined || r.round === round)).map(r => <p key={r.round} role="status" className="text-[var(--flag)]" data-testid={`contribution-${r.round}`}>{t('contributionExceeded', { n: r.newTextPct, m: snapshot!.policy.new_content_limit_pct })}</p>)}</>
}
