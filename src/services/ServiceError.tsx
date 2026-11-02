// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useStore } from '../state/store'
import { serviceErrors, snapshots } from './storeAdapter'
export const runServiceError = (id: string) => serviceErrors.get(id) ?? snapshots.get(id)?.lastError ?? serviceErrors.get('settings')
export function ServiceError({ id }: { id: string }) {
 useStore(s => s)
 const [dismissed, dismiss] = useState<unknown>()
 const error = runServiceError(id)
 if (!error || dismissed === error) return null
 return <ServiceErrorView error={error} onDismiss={() => dismiss(error)} />
}
export function ServiceErrorView({ error, onDismiss }: { error: unknown; onDismiss: () => void }) {
 const { t } = useTranslation()
 const message = error instanceof Error ? error.message : error && typeof error === 'object' && 'message' in error ? String(error.message) : String(error)
 const publication = error && typeof error === 'object' && 'kind' in error && error.kind === 'publication'
 return <div role="alert" className="mb-4 rounded-lg border border-[var(--flag)] p-3">{publication ? message : t('service.error', { message })}<button type="button" className="ms-3" onClick={onDismiss}>{t('common.close')}</button></div>
}
