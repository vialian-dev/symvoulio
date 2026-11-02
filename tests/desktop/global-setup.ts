// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Starts the vite dev server the debug app loads (http://localhost:5173) and returns its teardown (by PID).
import { spawn } from 'node:child_process'
import { join, resolve } from 'node:path'
import { readdirSync, rmSync, mkdirSync, openSync } from 'node:fs'
import { tmpdir } from 'node:os'

async function up(url: string, ms: number) {
  const end = Date.now() + ms
  while (Date.now() < end) {
    try { if ((await fetch(url)).ok) return true } catch { /* not yet */ }
    await new Promise((r) => setTimeout(r, 300))
  }
  return false
}

export default async function globalSetup() {
  // A release-frontend build loads its bundled dist through the custom protocol: no dev server is started.
  if (process.env.SYMVOULIO_E2E_BASE || process.env.SYMVOULIO_DEV_BASE) return async () => {}
  if (await up('http://localhost:5173/', 500)) throw new Error('port 5173 is already in use: stop that server first')
  // The dev server's own log is kept per run (test-results/vite-<UTC>.log): a dependency re-optimisation makes vite
  // reload every page, which a test sees as «execution context was destroyed» or a lost in-memory store.
  mkdirSync('test-results', { recursive: true })
  const viteLog = openSync(join('test-results', `vite-${new Date().toISOString().replace(/[:.]/g, '-')}.log`), 'a')
  const vite = spawn(process.execPath, [resolve('node_modules/vite/bin/vite.js'), '--port', '5173', '--strictPort'], { stdio: ['ignore', viteLog, viteLog] })
  if (!(await up('http://localhost:5173/', 30_000))) { vite.kill(); throw new Error('vite dev server did not start') }
  return async () => {
    vite.kill()
    // Sweep the isolated data dirs a test could not remove while WebView2 still held a file.
    await new Promise((r) => setTimeout(r, 1500))
    if (!process.env.SYMVOULIO_E2E_KEEP) for (const d of readdirSync(tmpdir())) if (d.startsWith('symvoulio-e2e-')) {
      try { rmSync(join(tmpdir(), d), { recursive: true, force: true, maxRetries: 5, retryDelay: 500 }) } catch { /* still locked */ }
    }
  }
}
