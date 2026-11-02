// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// CARD R07b (cruel tester): launchApp must never hand back a page that sits on the WebView2 error page.
// When the vite dev server does not answer the app's first request in time, WebView2 shows chrome-error://chromewebdata/,
// where '__TAURI_INTERNALS__' in window is ALSO true (Tauri injects it into every document). A harness that waits only for
// that proceeds on the error page, and the next evaluate dies with «Execution context was destroyed» once the real page loads.
//
// Rule tested: launchApp either resolves with a page on the real origin (never chrome-error) on which five consecutive
// evaluates succeed, or rejects with an Error that names the error page. Own config (no globalSetup):
//   npx playwright test -c playwright.desktop.r07b.config.ts
import { test, expect } from '@playwright/test'
import { spawn, execFileSync, type ChildProcess } from 'node:child_process'
import { join, resolve } from 'node:path'
import http from 'node:http'
import net from 'node:net'
import { launchApp, BASE, type App } from './harness'

const DELAY = 35_000

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))
function listening(port: number): Promise<boolean> {
  return new Promise((ok) => {
    const s = net.connect({ port, host: '127.0.0.1' })
    s.once('connect', () => { s.destroy(); ok(true) })
    s.once('error', () => ok(false))
  })
}
async function requireFreePort(port: number) {
  if (await listening(port)) throw new Error(`port ${port} is already in use: stop that server first`)
}
function startVite(port: number): ChildProcess {
  return spawn(process.execPath, [resolve('node_modules/vite/bin/vite.js'), '--port', String(port), '--strictPort'], { stdio: 'ignore' })
}
function kill(p?: ChildProcess) {
  if (p?.pid && p.exitCode === null) {
    try { execFileSync('taskkill', ['/T', '/F', '/PID', String(p.pid)], { stdio: 'ignore' }) } catch { /* gone */ }
  }
}
async function waitUp(port: number, ms: number) {
  const end = Date.now() + ms
  while (Date.now() < end) {
    try { if ((await fetch(`http://localhost:${port}/`)).ok) return } catch { /* not yet */ }
    await sleep(300)
  }
  throw new Error(`vite on ${port} did not start`)
}

/** The rule. A rejection must name the error page; a resolved page must be real and stay usable. */
async function expectReadyOrNamedError(launch: () => Promise<App>): Promise<App | undefined> {
  let app: App
  try {
    app = await launch()
  } catch (e) {
    expect(String((e as Error).message), 'launchApp rejected without naming the error page').toMatch(/chrome-error|error page/i)
    return undefined
  }
  const url = app.page.url()
  expect(url, `launchApp handed back a page on ${url}`).not.toMatch(/^chrome-error:/)
  expect(url.startsWith('http://localhost:5173'), `page is not on the real origin: ${url}`).toBe(true)
  expect(await app.page.evaluate(() => '__TAURI_INTERNALS__' in window)).toBe(true)
  for (let i = 1; i <= 5; i++) {
    const href = await app.page.evaluate(() => location.href).catch((e) => { throw new Error(`evaluate ${i}/5 failed: ${String(e.message).split('\n')[0]}`) })
    expect(href, `evaluate ${i}/5 ran on ${href}`).not.toMatch(/^chrome-error:/)
  }
  return app
}

test.describe('R07b harness readiness', () => {
  test('refused start: the dev server comes up 35 s after launchApp', async () => {
    test.setTimeout(240_000)
    await requireFreePort(5173)
    let vite: ChildProcess | undefined
    let app: App | undefined
    const timer = setTimeout(() => { vite = startVite(5173) }, DELAY)
    try {
      app = await expectReadyOrNamedError(() => launchApp())
    } finally {
      clearTimeout(timer)
      await app?.stop()
      kill(vite)
    }
  })

  test('held start: every request on 5173 is held 35 s before it is forwarded to vite on 5174', async () => {
    test.setTimeout(240_000)
    await requireFreePort(5173)
    await requireFreePort(5174)
    const vite = startVite(5174)
    const t0 = Date.now()
    const proxy = http.createServer((req, res) => {
      setTimeout(() => {
        const up = http.request({ host: '127.0.0.1', port: 5174, path: req.url, method: req.method, headers: req.headers }, (u) => {
          res.writeHead(u.statusCode ?? 502, u.headers); u.pipe(res)
        })
        up.on('error', () => { res.writeHead(502); res.end() })
        req.pipe(up)
      }, Math.max(0, DELAY - (Date.now() - t0)))
    })
    let app: App | undefined
    try {
      await waitUp(5174, 60_000)
      await new Promise<void>((ok, bad) => { proxy.once('error', bad); proxy.listen(5173, ok) })
      app = await expectReadyOrNamedError(() => launchApp())
    } finally {
      await app?.stop()
      proxy.closeAllConnections?.()
      proxy.close()
      kill(vite)
    }
  })

  test('control: vite is up before launchApp', async () => {
    test.setTimeout(240_000)
    await requireFreePort(5173)
    const vite = startVite(5173)
    let app: App | undefined
    try {
      await waitUp(5173, 60_000)
      app = await launchApp()
      expect(app.page.url().startsWith(BASE)).toBe(true)
      expect(app.page.url()).not.toMatch(/^chrome-error:/)
      expect(await app.page.evaluate(() => '__TAURI_INTERNALS__' in window)).toBe(true)
    } finally {
      await app?.stop()
      kill(vite)
    }
  })
})
