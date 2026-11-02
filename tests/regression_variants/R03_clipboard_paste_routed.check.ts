// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Execute the real Terminal effect and desktop IPC adapter with an EventTarget host.
// No DOM renderer or provider process is needed; xterm and React lifecycle are harnessed.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { createJiti } from 'jiti'
const compiler = createJiti(import.meta.url, { fsCache: false })
import { pasteDecision } from '../../src/lib/terminalInput'
import { PALETTE, xtermTheme } from '../../src/lib/terminalTheme'

function load(file: string, dependencies: Record<string, unknown>) {
  const source = readFileSync(resolve(file), 'utf8')
  const js = compiler.transform({ source, filename: resolve(file), ts: true, jsx: { runtime: 'automatic' } })
  const exports: Record<string, any> = {}
  const require = (name: string) => {
    assert.ok(name in dependencies, `unexpected dependency: ${name}`)
    return dependencies[name]
  }
  new Function('require', 'exports', 'module', 'jitiImport', js)(require, exports, { exports }, async (name: string) => require(name))
  return exports
}

async function main() {
  let refuse = false
  const written: string[] = []
  let release: (() => void) | undefined
  const calls: { command: string; args: unknown }[] = []
  const { desktopBackend } = load('src/services/backend.ts', {
    './canned': { cannedBackend: {} }, './env': { isDesktop: true },
    '@tauri-apps/api/core': { invoke: async (command: string, args: unknown) => { calls.push({ command, args }); if (command === 'pty_write') { const data = (args as {data:string}).data; if (data === 'c') await new Promise<void>(r => { release = r }); written.push(data) }; if (refuse) throw new Error('clipboard backend refused') } },
  })
  const host = new EventTarget()
  const effects: (() => unknown)[] = []
  const displayed: string[] = []
  let onData: (text: string) => void = () => { throw new Error('onData not installed') }
  class XTerm {
    options = {}; cols = 80; rows = 24
    constructor(options: { ignoreBracketedPasteMode: boolean }) { assert.equal(options.ignoreBracketedPasteMode, true) }
    loadAddon() {} open() {} dispose() {}
    onData(callback: typeof onData) { onData = callback }
    write(text: string) { displayed.push(text) }
  }
  class Observer { observe() {} disconnect() {} }
  const savedDocument = globalThis.document
  const savedMutation = globalThis.MutationObserver
  const savedResize = globalThis.ResizeObserver
  try {
    Object.assign(globalThis, { document: { documentElement: { classList: { contains: () => false } } }, MutationObserver: Observer, ResizeObserver: Observer })
    const { Terminal } = load('src/frame/Terminal.tsx', {
      'react': { useEffect: (fn: () => unknown) => effects.push(fn), useRef: (value: unknown) => ({ current: value }), useState: () => [false] },
      'react/jsx-runtime': { jsx: (_: unknown, props: { ref: { current: unknown } }) => { props.ref.current = host } },
      '@xterm/xterm': { Terminal: XTerm }, '@xterm/addon-fit': { FitAddon: class { fit() {} } },
      '../services/panelShells': { setPanelShell() {} }, '../services/backend': { backend: desktopBackend },
      '../lib/terminalTheme': { PALETTE, xtermTheme },
      '../lib/terminalInput': { pasteDecision }, '../state/install': { installExited() {}, installPainted() {} }, '../i18n': { i18n: { t: () => 'paste refused' } },
    })
    Terminal({ lines: [], mode: 'raw', prompt: null, pending: null, label: 'test', testId: 'test', contextKey: 'test', live: 'claude' })
    // Run mount effects through xterm input registration; later effects attach a real PTY/render output.
    const cleanups = effects.slice(0, 2).map(fn => fn())
    async function paste(text: string) {
      const event = new Event('paste', { cancelable: true, bubbles: true })
      Object.defineProperty(event, 'clipboardData', { value: { getData: () => text } })
      // Simulate xterm's bubbling fallback: cancellation must stop raw onData forwarding.
      host.addEventListener('paste', () => { if (!event.defaultPrevented) onData(text) }, { once: true })
      host.dispatchEvent(event)
      await new Promise(resolve => setTimeout(resolve, 0))
      assert.equal(event.defaultPrevented, true, 'clipboard event must be cancelled')
    }
    await paste('echo clipboard-marker')
    assert.deepEqual(calls, [{ command: 'pty_paste', args: { panel: 'claude', command: 'echo clipboard-marker', guard: undefined, clipboard: true } }], 'safe clipboard must reach pty_paste')
    for (const unsafe of ['echo unsafe\n', 'echo unsafe\r', 'echo unsafe\x03', '\x1b[200~bad']) await paste(unsafe)
    assert.equal(calls.length, 1, 'unsafe clipboard must be refused')
    assert.equal(displayed.filter(s => s.includes('paste refused')).length, 4)
    assert.ok(!calls.some(c => c.command === 'pty_write'), 'clipboard must never reach pty_write')
    // Verify the raw-key route is instrumented and can detect a clipboard bypass.
    onData('k'); await new Promise(resolve => setTimeout(resolve, 0))
    assert.deepEqual(calls[1], { command: 'pty_write', args: { panel: 'claude', data: 'k' } })
    onData('c'); onData('l'); onData('a')
    await new Promise(resolve => setTimeout(resolve, 0))
    assert.deepEqual(written, ['k'], 'later keys must wait for the blocked first IPC')
    release!()
    await new Promise(resolve => setTimeout(resolve, 0))
    assert.deepEqual(written, ['k', 'c', 'l', 'a'])
    refuse = true
    await paste('auth-code')
    assert.ok(displayed.some(s => s.includes('clipboard backend refused')), 'backend refusal must be visible')
    for (const cleanup of cleanups.reverse()) if (typeof cleanup === 'function') cleanup()
  } finally {
    Object.assign(globalThis, { document: savedDocument, MutationObserver: savedMutation, ResizeObserver: savedResize })
  }
  console.log('R03 event dispatch: PASS (safe pty_paste, unsafe refused, no clipboard pty_write)')
}
main().catch(error => { console.error(error); process.exitCode = 1 })

