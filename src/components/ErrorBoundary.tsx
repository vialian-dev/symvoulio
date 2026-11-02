// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// A calm error screen instead of a raw stack (a browser
// translation rewrites the DOM and React's next render throws «removeChild»). Two ways out: reload, or back to Reviews.
import { Component, type ErrorInfo, type ReactNode } from 'react'
import { i18n } from '../i18n'

export class ErrorBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false }
  static getDerivedStateFromError() { return { failed: true } }
  componentDidCatch(error: Error, info: ErrorInfo) {
    // Kept for diagnosis; a warning, not an error, so the screen itself is the visible outcome.
    console.warn('Symvoulio recovered from a render error:', error.message, info.componentStack?.split('\n')[1]?.trim())
  }
  render() {
    if (!this.state.failed) return this.props.children
    const t = i18n.t.bind(i18n)
    return (
      <div data-screen="error" className="grid h-full place-items-center bg-[var(--bg)] p-8">
        <div className="max-w-[480px] rounded-[var(--radius-card)] border border-[var(--line)] bg-[var(--surface)] p-8 text-center shadow-[var(--shadow-lift)]">
          <div className="serif-title text-[19px] font-bold tracking-[0.06em]">Symvoulio</div>
          <h1 data-testid="error-title" className="serif-title mt-4 text-[28px] font-semibold">{t('error.title')}</h1>
          <p className="mt-2 text-[13.5px] text-[var(--ink-2)]">{t('error.text')}</p>
          <div className="mt-6 flex justify-center gap-2">
            <button type="button" data-testid="error-reload" onClick={() => location.reload()}
              className="rounded-lg bg-[var(--accent)] px-4 py-2 text-[13px] font-medium text-[var(--on-accent)]">{t('error.reload')}</button>
            <button type="button" data-testid="error-back" onClick={() => { location.href = '/reviews' }}
              className="rounded-lg border border-[var(--line)] px-4 py-2 text-[13px] font-medium text-[var(--ink-2)]">{t('error.back')}</button>
          </div>
        </div>
      </div>
    )
  }
}
