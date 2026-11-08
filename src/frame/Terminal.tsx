// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// One provider terminal (xterm.js 6, DOM renderer). The prototype has no PTY: lines come from the store (canned run
// output or the stub shell of the wizard). Readable / Raw re-renders the same lines in the other form. A command typed
// by «Paste command» sits at the prompt unexecuted; Enter typed by the user in this terminal runs the stub.
import { useEffect, useRef, useState } from 'react'
import { Terminal as XTerm } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import type { TermLine } from '../lib/script'
import type { Provider } from '../data/sample'
import { registerPanelReset, setPanelShell } from '../services/panelShells'
import { getState } from '../state/store'
import { backend, closePanel } from '../services/backend'
import { pasteDecision } from '../lib/terminalInput'
import { installExited, installPainted, installTimeout } from '../state/install'
import { i18n } from '../i18n'

// Two palettes: the terminal follows the app theme (light by default, dark on the toggle).
const rgb = (h: string) => `\x1b[38;2;${parseInt(h.slice(1, 3), 16)};${parseInt(h.slice(3, 5), 16)};${parseInt(h.slice(5, 7), 16)}m`
import { PALETTE, xtermTheme, type Pal } from '../lib/terminalTheme'

const isDark = () => document.documentElement.classList.contains('dark')
const reset = '\x1b[0m'

/** Raw lines are JSON events: keys, strings, numbers and punctuation in their own tones (Figma S07r). */
function colorJson(r: string, P: Pal): string {
  return r.replace(/("(?:[^"\\]|\\.)*")(\s*:)?|(-?\d+(?:\.\d+)?)|([{}[\],])|\b(true|false|null)\b/g, (m, str?: string, colon?: string, num?: string, punct?: string, lit?: string) => {
    if (str) return colon ? `${rgb(P.key)}${str}${reset}${rgb(P.punct)}${colon}${reset}` : `${rgb(P.str)}${str}${reset}`
    if (num) return `${rgb(P.num)}${num}${reset}`
    if (punct) return `${rgb(P.punct)}${punct}${reset}`
    if (lit) return `${rgb(P.num)}${lit}${reset}`
    return m
  })
}

function render(line: TermLine, mode: 'readable' | 'raw', P: Pal): string {
  const C = { reset, accent: rgb(P.accent), ok: rgb(P.ok), err: rgb(P.err), ink: rgb(P.ink), gold: rgb(P.gold) }
  const text = mode === 'raw' ? line.raw : line.readable
  const rows = text.split('\n')
  return rows.map((r) => {
    if (line.tone === 'cmd') return `${C.ink}${r}${C.reset}`
    if (mode === 'raw') return r.startsWith('> ') ? `${C.gold}${r}${C.reset}` : colorJson(r, P)
    if (line.tone === 'done') return `${C.ok}  ✓ ${r}${C.reset}`
    if (line.tone === 'error') return `${C.err}  ✕ ${r}${C.reset}`
    if (line.tone === 'out') return `${C.ink}${r}${C.reset}`
    return `${C.accent}  › ${C.reset}${r}`
  }).join('\r\n')
}


export function Terminal({ lines, mode, prompt, pending, onEnter, label, testId, contextKey, live }: {
  contextKey: string; lines: TermLine[]; mode: 'readable' | 'raw'; prompt: string | null; pending: string | null; onEnter?: () => void; label: string; testId: string
  /** Desktop app with no run open: this panel is the provider's real shell (a PTY); the canned props are ignored. */
  live?: Provider
}) {
  const host = useRef<HTMLDivElement>(null)
  const term = useRef<XTerm | null>(null)
  const fit = useRef<FitAddon | null>(null)
  const enter = useRef(onEnter)
  enter.current = onEnter
  const liveRef = useRef(live)
  liveRef.current = live
  // The last drawing; a width change redraws it whole (a wrapped command at the prompt must not lose its tail).
  const draw = useRef<(() => void) | null>(null)
  const [dark, setDark] = useState(isDark)
  const [stalled, setStalled] = useState(false)
  const [reopen, setReopen] = useState(0)
  const panelGeneration = useRef(0)
  // F2/F4: callers of resetPanel wait here until the fresh shell is attached.
  const resetWaiters = useRef<{ done: () => void; fail: (e: unknown) => void }[]>([])
  // Follow the theme toggle (the `dark` class on <html>).
  useEffect(() => {
    const mo = new MutationObserver(() => setDark(isDark()))
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] })
    return () => mo.disconnect()
  }, [])

  useEffect(() => {
    const t = new XTerm({
      fontFamily: '"JetBrains Mono", "Cascadia Mono", Consolas, monospace', fontSize: 12.5, lineHeight: 1.25,
      // Also correct low-contrast truecolor output from provider CLIs.
      minimumContrastRatio: 9, ignoreBracketedPasteMode: true, cursorBlink: true, cursorInactiveStyle: 'none', convertEol: true, scrollback: 2000, allowProposedApi: false,
      theme: xtermTheme(isDark() ? PALETTE.dark : PALETTE.light),
    })
    const f = new FitAddon()
    t.loadAddon(f)
    t.open(host.current!)
    const element = host.current!
    let inputQueue = Promise.resolve()
    const onPaste = (event: ClipboardEvent) => {
      const panel = liveRef.current
      if (!panel) return
      event.preventDefault()
      event.stopPropagation()
      const text = event.clipboardData?.getData('text/plain') ?? ''
      if (pasteDecision(text) === 'paste') inputQueue = inputQueue.then(() => backend.paste(panel, text, undefined, true)).catch(e => t.write(`\r\n${String(e)}\r\n`))
      else t.write(`\r\n\x1b[2m${i18n.t('terminal.pasteRefused')}\x1b[0m\r\n`)
    }
    element.addEventListener('paste', onPaste, true)
    // Let the browser dispatch paste before xterm consumes Ctrl/Cmd+V as raw input.
    // Preserve the default action: the paste event still uses the guarded handler above.
    const onPasteShortcut = (event: KeyboardEvent) => {
      if (liveRef.current && !event.altKey && (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'v') event.stopPropagation()
    }
    element.addEventListener('keydown', onPasteShortcut, true)
    // Live PTY: every key the user types goes to the shell as is (only the user ever sends Enter). Canned: Enter runs the stub.
    // IPC requests may complete out of order. Keep user keys in their original order,
    // including Enter, and keep the queue usable after a visible write failure.
    t.onData((d) => {
      const panel = liveRef.current
      if (panel) inputQueue = inputQueue.then(() => backend.ptyWrite(panel, d)).catch(e => t.write(`\r\n${String(e)}\r\n`))
      else if (d === '\r') enter.current?.()
    })
    term.current = t
    fit.current = f
    const ro = new ResizeObserver(() => {
      try {
        if (!host.current || host.current.clientHeight === 0) return
        const cols = t.cols
        const rows = t.rows
        f.fit()
        if (liveRef.current) { if (t.cols !== cols || t.rows !== rows) void backend.ptyResize(liveRef.current, t.cols, t.rows).catch(() => { /* The shell may not have attached yet. */ }) }
        else if (t.cols !== cols) draw.current?.()
      } catch { /* not laid out yet */ }
    })
    ro.observe(host.current!)
    return () => { element.removeEventListener('paste', onPaste, true); element.removeEventListener('keydown', onPasteShortcut, true); ro.disconnect(); t.dispose(); term.current = null; draw.current = null }
  }, [])

  useEffect(() => { term.current?.textarea?.setAttribute('aria-label', label) }, [label])
  useEffect(() => { if (term.current) term.current.options.theme = xtermTheme(dark ? PALETTE.dark : PALETTE.light) }, [dark])

  // Live PTY: attach (or re-attach, which replays the shell's recent output) while this panel is live.
  useEffect(() => {
    const t = term.current
    if (!t || !live) return
    let attached = true
    draw.current = null
    t.options.windowsPty = navigator.userAgent.includes('Windows') ? { backend: 'conpty' } : undefined
    t.write('\x1bc')
    // The backend keeps the LAST attached sink per panel. A mount that is undone at once (React StrictMode, fast
    // navigation) must not attach, or its call could land after the real one and steal the output: attach on a tick.
    const openPanel = () => {
      const generation = ++panelGeneration.current
      const current = () => attached && generation === panelGeneration.current
      void backend.getSettings().then(settings => current() ? backend.ptyOpen(live, undefined, t.cols, t.rows, (e) => {
        if (!current()) return
        if (e.kind === 'Data') t.write(e.data)
        if (e.kind === 'Exit') { setPanelShell(live); installTimeout(live); setStalled(true) }
        // A sign-in waits for the browser for minutes; that is not a stuck terminal (F2, F4).
        if (e.kind === 'CommandTimeout') { installTimeout(live); if (!getState().signingIn[live]) setStalled(true) }
        if (e.kind === 'CommandExit') void installExited(live, e.code, e.last).then(installed => { if (current() && installed) { t.write('\x1bc'); openPanel() } })
      }, settings.shell[live] || undefined) : undefined)
        .then(info => { if (current() && info) { setPanelShell(live, info.shell); setStalled(false); resetWaiters.current.splice(0).forEach(w => w.done()) } })
        .catch((e) => { resetWaiters.current.splice(0).forEach(w => w.fail(e)); if (current()) t.write(`\r\n${String(e)}\r\n`) })
    }
    const timer = setTimeout(openPanel, 30)
    return () => { attached = false; clearTimeout(timer); t.options.windowsPty = undefined }
  }, [live, reopen])

  // F2/F4: a clean shell on request: end the panel's process, then open a fresh shell (the effect above clears the screen).
  useEffect(() => {
    if (!live) return
    return registerPanelReset(live, () => new Promise<void>((done, fail) => {
      resetWaiters.current.push({ done, fail })
      void closePanel(live).then(() => setReopen(n => n + 1)).catch((e) => { resetWaiters.current.splice(0).forEach(w => w.fail(e)) })
    }))
  }, [live])

  useEffect(() => {
    const t = term.current
    if (!t || live) return
    const P = dark ? PALETTE.dark : PALETTE.light
    const C = { ink: rgb(P.ink), reset }
    // Always a whole redraw at the current width: xterm then wraps every line, including the command waiting at the
    // prompt, so the user always sees the full command before pressing Enter. A few hundred lines; cheap.
    // One write, starting with a full terminal reset (RIS) inside the same stream: a separate reset() would run before
    // earlier queued writes and print them twice.
    const redraw = () => {
      let out = '\x1bc'
      // Readable skips lines that only have a raw form (desktop runs); canned lines always have both.
      for (const l of lines) if (mode === 'raw' || l.readable) out += render(l, mode, P) + '\r\n'
      if (prompt !== null) out += `${C.ink}${prompt}${C.reset}${pending ?? ''}`
      t.write(out, () => { if (pending) installPainted(testId) })
    }
    draw.current = redraw
    redraw()
  }, [lines, mode, prompt, pending, contextKey, dark, live])

  return <div ref={host} data-testid={testId} className="relative h-full min-h-0 w-full bg-[var(--term-bg)]">
    {live && stalled && <div role="status" className="absolute inset-x-0 bottom-0 bg-[var(--surface)] p-2 text-[12px]">
      {i18n.t('terminal.busyHint')} <button data-testid={`reopen-${live}`} className="underline" onClick={() => { void closePanel(live).then(() => setReopen(n => n + 1)).catch(e => term.current?.write(`\r\n${String(e)}\r\n`)) }}>{i18n.t('terminal.reopen')}</button>
    </div>}
  </div>
}
