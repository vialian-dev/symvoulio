// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// S2 Connect Claude Code / S3 Connect Codex: a five-step guide with a screenshot slot per step. «Paste command»
// types the official login command into the provider's panel WITHOUT running it; the user presses Enter there. «Check
// connection» pastes the status command and reads the stubbed status. The app stores only connected yes/no and the
// sign-in method — never an email, organisation or credential.
// 0.3.1 on macOS and Linux (Windows keeps the flow above unchanged): account sign-in is four steps without picture
// slots; «Sign in» runs the official login in the panel (the app presses Enter) and the status turns connected by itself,
// then the panel is reset (F2, F4); the help for the step is at the bottom of the left rail (F5).
import { apiKeyLogin } from '../lib/shellCommands'
import { usePanelShell } from '../services/panelShells'
import { isDesktop } from '../services/env'
import { useState, type ReactNode } from 'react'
import { useNavigate, useSearchParams } from 'react-router'
import { useTranslation } from 'react-i18next'
import { AnimatePresence, motion } from 'motion/react'
import { ArrowLeft, ArrowRight, CheckCircle2, ClipboardPaste, CornerDownLeft, Globe, ImageIcon, Info, KeyRound, LogIn, PlugZap, ShieldCheck } from 'lucide-react'
import { Button, Card, cx } from '@symvoulio/ui/components'
import { Page, Stepper } from '../components/Page'
import { Segmented } from '../components/ui/controls'
import type { Provider } from '../data/sample'
import { cancelSignIn, checkConnection, clearError, pasteCommand, setSignInMethod, signIn, useStore } from '../state/store'
import { appSignIn, usePlatform } from '../services/platform'

const CMD: Record<Provider, { login: string; status: string }> = {
  claude: { login: 'claude auth login', status: 'claude auth status' },
  codex: { login: 'codex login', status: 'codex login status' },
}

function Shot({ caption, icon }: { caption: string; icon: ReactNode }) {
  return (
    // the slot grows with its caption (no fixed ratio), and the picture mark sits in the flow, never over the text.
    <div data-testid="shot-slot" className="flex min-h-[96px] w-full flex-col items-center justify-center gap-1.5 rounded-lg border border-dashed border-[var(--line)] bg-[radial-gradient(120%_90%_at_0%_0%,var(--accent-soft),transparent_60%),var(--surface-2)] px-2 py-2.5 text-center text-[var(--muted)]">
      <span className="grid size-9 shrink-0 place-items-center rounded-full bg-[var(--surface)] text-[var(--accent)] shadow-[var(--shadow-card)]">{icon}</span>
      <span data-testid="shot-caption" className="text-[10.5px] leading-tight">{caption}</span>
      <ImageIcon data-testid="shot-mark" size={12} className="shrink-0 text-[var(--muted)]" aria-hidden />
    </div>
  )
}

export function Connect({ provider: p }: { provider: Provider }) {
  const { t } = useTranslation()
  const nav = useNavigate()
  const [params] = useSearchParams()
  const returnTo = params.get('return')
  const conn = useStore((s) => s.conn[p])
  const shell = useStore((s) => s.shells[p])
  const preferred = useStore((s) => s.shellName[p])
  const actualShell = usePanelShell(p)
  const loginShell = isDesktop ? actualShell : preferred
  const method = useStore((s) => s.signInMethod[p])
  const setMethod = (m: 'subscription' | 'api_key') => setSignInMethod(p, m)
  const mac = appSignIn(usePlatform())
  const signingIn = useStore((s) => s.signingIn[p])
  const [checking, setChecking] = useState(false)
  const [result, setResult] = useState<'ok' | 'no' | null>(null)
  const name = t(`provider.${p}`)
  const loginCmd = method === 'subscription' ? CMD[p].login : loginShell ? apiKeyLogin(p, loginShell) : ''
  const displayCmd = loginCmd || apiKeyLogin(p, 'powershell')
  const pasted = shell.pending === loginCmd

  const check = async () => {
    setChecking(true)
    const ok = await checkConnection(p)
    setChecking(false)
    setResult(ok ? 'ok' : 'no')
  }

  const steps: { n: number; title: string; text: ReactNode; action?: ReactNode; shot: string; icon: ReactNode }[] = [
    { n: 1, title: t('s2.s1.title'), text: t(`s2.s1.text.${p}`, { name }), shot: t('s2.shot.panel', { name }), icon: <Info size={16} /> },
    {
      n: 2, title: t('s2.s2.title'), text: <>{t('s2.s2.text', { name })} <code className="rounded bg-[var(--term-bg)] px-1.5 py-0.5 font-[family-name:var(--font-mono)] text-[12px] text-[var(--ink-2)]">{displayCmd}</code>{!loginCmd && <span role="status"> {t('s2.shellRequired')}</span>}</>,
      action: <Button data-testid="paste-login" disabled={!loginCmd} onClick={() => pasteCommand(p, loginCmd, method === 'subscription' ? 'login' : 'login_key')}><ClipboardPaste size={14} />{t('s2.paste')}</Button>,
      shot: t('s2.shot.typed'), icon: <ClipboardPaste size={16} />,
    },
    { n: 3, title: t('s2.s3.title'), text: t('s2.s3.text', { name }), shot: t('s2.shot.enter'), icon: <CornerDownLeft size={16} /> },
    { n: 4, title: t(`s2.s4.title.${method}`), text: t(`s2.s4.text.${p}.${method}`), shot: t(`s2.shot.${method}`), icon: method === 'subscription' ? <Globe size={16} /> : <KeyRound size={16} /> },
    {
      n: 5, title: t('s2.s5.title'), text: <>{t('s2.s5.text')} <code className="rounded bg-[var(--term-bg)] px-1.5 py-0.5 font-[family-name:var(--font-mono)] text-[12px] text-[var(--ink-2)]">{CMD[p].status}</code></>,
      action: <Button data-testid="check-connection" variant="primary" disabled={checking} onClick={() => void check()}><PlugZap size={14} />{checking ? t('s2.checking') : t('s2.check')}</Button>,
      shot: t('s2.shot.status'), icon: <ShieldCheck size={16} />,
    },
  ]
  const code = (text: string) => <code className="rounded bg-[var(--term-bg)] px-1.5 py-0.5 font-[family-name:var(--font-mono)] text-[12px] text-[var(--ink-2)]">{text}</code>
  const paste = <Button data-testid="paste-login" variant={method === 'subscription' ? 'quiet' : undefined} disabled={!loginCmd || signingIn} onClick={() => pasteCommand(p, loginCmd, method === 'subscription' ? 'login' : 'login_key')}><ClipboardPaste size={14} />{t('s2.paste')}</Button>
  const checkStep = (n: number) => ({
    n, title: t('s2.s5.title'), text: <>{t(method === 'subscription' ? 's2.s5.textAuto' : 's2.s5.text')} {code(CMD[p].status)}</>,
    action: <Button data-testid="check-connection" variant="primary" disabled={checking} onClick={() => void check()}><PlugZap size={14} />{checking ? t('s2.checking') : t('s2.check')}</Button>,
  })
  // macOS: account sign-in in four steps, the app runs the login (F2); API key: the five steps, the user runs it.
  const macSteps: { n: number; title: string; text: ReactNode; action?: ReactNode }[] = method === 'subscription' ? [
    { n: 1, title: t('s2.s1.title'), text: t(`s2.s1.textAuto.${p}`, { name }) },
    {
      n: 2, title: t('s2.signIn.title'), text: <>{t('s2.signIn.text', { name })} {code(CMD[p].login)}</>,
      action: <div className="flex flex-wrap items-center gap-2">
        <Button data-testid="sign-in" variant="primary" disabled={signingIn || conn.connected || !conn.installed} onClick={() => void signIn(p)}><LogIn size={14} />{signingIn ? t('s2.signIn.waiting') : t('s2.signIn.button')}</Button>
        {signingIn ? <Button data-testid="sign-in-cancel" variant="quiet" onClick={() => cancelSignIn(p)}>{t('common.cancel')}</Button> : paste}
      </div>,
    },
    { n: 3, title: t('s2.s4.title.subscription'), text: t(`s2.s4.text.${p}.subscription`) },
    checkStep(4),
  ] : [
    { n: 1, title: t('s2.s1.title'), text: t(`s2.s1.text.${p}`, { name }) },
    { n: 2, title: t('s2.s2.title'), text: <>{t('s2.s2.text', { name })} {code(displayCmd)}{!loginCmd && <span role="status"> {t('s2.shellRequired')}</span>}</>, action: paste },
    { n: 3, title: t('s2.s3.title'), text: t('s2.s3.text', { name }) },
    { n: 4, title: t('s2.s4.title.api_key'), text: t(`s2.s4.text.${p}.api_key`) },
    checkStep(5),
  ]

  return (
    <Page id={p === 'claude' ? 'S2' : 'S3'} caption={t('setup.caption')} title={t('s2.title')} accent={name}>
      <Stepper current={p} />
      <div className="mb-4 flex flex-wrap items-center gap-3">
        <span className="text-[13px] text-[var(--muted)]">{t('s2.signInWith')}</span>
        <Segmented label={t('s2.signInWith')} value={method} onChange={setMethod} testId="method"
          options={[{ value: 'subscription', label: t(`s2.method.subscription.${p}`), testId: 'method-subscription' }, { value: 'api_key', label: t('s2.method.api_key'), testId: 'method-api-key' }]} />
      </div>

      <AnimatePresence>
        {conn.connected && (
          <motion.div initial={{ opacity: 0, y: -6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0 }}>
            <Card data-testid="connected" className="mb-4 flex items-center gap-3 border-[var(--ok)] px-5 py-3">
              <CheckCircle2 className="text-[var(--ok)]" size={22} />
              <div>
                <div className="text-[15px] font-semibold">{t('s2.connected', { name })}</div>
                <div className="text-[12px] text-[var(--muted)]">{t('s2.connectedMethod', { method: t(`s2.methodName.${conn.method ?? 'subscription'}`) })}</div>
              </div>
            </Card>
          </motion.div>
        )}
      </AnimatePresence>
      {result === 'no' && !conn.connected && (
        <p data-testid="not-connected" role="status" className="mb-4 rounded-lg border border-[var(--value)] bg-[var(--value-soft)] px-4 py-2 text-[13px] text-[var(--value-ink)]">{t(mac && method === 'subscription' ? 's2.notYetAuto' : 's2.notYet')}</p>
      )}

      {mac ? <ol className="relative space-y-3">
        {macSteps.map((s) => (
          <li key={s.n} data-testid={`guide-step-${s.n}`}>
            <Card className={cx('grid grid-cols-[36px_minmax(0,1fr)] gap-4 p-4', s.n === 2 && (pasted || signingIn) && 'border-[var(--accent)]')}>
              <span className="grid size-8 place-items-center rounded-full bg-[var(--accent-soft)] text-[13px] font-semibold text-[var(--accent)]">{s.n}</span>
              <div className="min-w-0">
                <h3 className="text-[14.5px] font-semibold">{s.title}</h3>
                <p className="mt-1 text-[13px] leading-relaxed text-[var(--ink-2)]">{s.text}</p>
                {s.action ? <div className="mt-3">{s.action}</div> : null}
                {s.n === 2 && shell.pasteError ? <p role="alert" className="mt-2 text-[12px] text-[var(--error)]">{shell.pasteError}</p> : null}
                {s.n === 2 && pasted ? <p data-testid="typed-hint" className="mt-2 text-[12px] text-[var(--accent)]">{t('s2.typedHint')}</p> : null}
                {s.n === 2 && signingIn ? <p data-testid="signing-in" role="status" className="mt-2 text-[12px] text-[var(--accent)]">{t('s2.signIn.hint', { name })}</p> : null}
              </div>
            </Card>
          </li>
        ))}
      </ol> : <ol className="relative space-y-3">
        {steps.map((s) => (
          <li key={s.n} data-testid={`guide-step-${s.n}`}>
            <Card className={cx('grid grid-cols-[36px_minmax(0,1fr)_168px] gap-4 p-4 max-[1400px]:grid-cols-[36px_minmax(0,1fr)_144px]', s.n === 2 && pasted && 'border-[var(--accent)]')}>
              <span className="grid size-8 place-items-center rounded-full bg-[var(--accent-soft)] text-[13px] font-semibold text-[var(--accent)]">{s.n}</span>
              <div className="min-w-0">
                <h3 className="text-[14.5px] font-semibold">{s.title}</h3>
                <p className="mt-1 text-[13px] leading-relaxed text-[var(--ink-2)]">{s.text}</p>
                {s.action ? <div className="mt-3">{s.action}</div> : null}
                {s.n === 2 && shell.pasteError ? <p role="alert" className="mt-2 text-[12px] text-[var(--error)]">{shell.pasteError}</p> : null}
                {s.n === 2 && pasted ? <p data-testid="typed-hint" className="mt-2 text-[12px] text-[var(--accent)]">{t('s2.typedHint')}</p> : null}
              </div>
              <Shot caption={s.shot} icon={s.icon} />
            </Card>
          </li>
        ))}
      </ol>}
      <p className="mt-4 flex items-center gap-2 text-[12.5px] text-[var(--muted)]"><ShieldCheck size={14} />{t(mac ? 's2.neverAuto' : 's2.never')}</p>

      <div className="mt-6 flex items-center gap-2">
        <Button data-testid="back" onClick={() => nav(p === 'claude' ? '/setup/system' : '/setup/claude')}><ArrowLeft size={14} />{t('common.back')}</Button>
        <span className="flex-1" />
        {returnTo && conn.connected ? (
          <Button data-testid="return-to-review" variant="primary" onClick={() => { clearError(returnTo); nav(`/review/${returnTo}`) }}>{t('s2.returnToReview')}</Button>
        ) : null}
        <Button data-testid="next" variant={returnTo ? 'ghost' : 'primary'} onClick={() => nav(p === 'claude' ? '/setup/codex' : '/setup/roles')}>{t('common.next')}<ArrowRight size={14} /></Button>
      </div>
    </Page>
  )
}
