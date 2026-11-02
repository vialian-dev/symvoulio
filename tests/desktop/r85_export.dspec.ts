// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { test, expect } from '@playwright/test'
import { existsSync, renameSync } from 'node:fs'
import { join } from 'node:path'
import { launchApp, ipc, BASE, type App } from './harness'
import type { RunSnapshot } from '../../src/services/types'

// Real engine/IPC/files, with only run_get delivery controlled by CDP.
async function completedReview(a: App) {
  const id = await a.page.evaluate(async () => {
    const ti = (window as any).__TAURI_INTERNALS__
    const file = await ti.invoke('stage_file', { name: 'plan.md', bytes: Array.from(new TextEncoder().encode('# Plan\n\nA short plan for review.')) })
    const settings = await ti.invoke('settings_get')
    const s = await ti.invoke('run_start', {
      input: { name: 'R85 real review', objective: 'Check facts', criteria: ['Facts supported'], policy: { ...settings.policy, max_rounds: 1, min_rounds: 1 }, author: 'claude', primaryToken: file.token, supportingTokens: [] },
      onEvent: `__CHANNEL__:${ti.transformCallback(() => {})}`,
    })
    return s.id as string
  })
  await expect.poll(async () => (await ipc<RunSnapshot>(a.page, 'run_get', { runId: id })).state, { timeout: 60_000 }).not.toBe('running')
  return id
}

for (const mode of ['delayed', 'failed', 'loaded'] as const) {
  test(`R85: ${mode} desktop snapshot never fabricates export success`, async () => {
    const a = await launchApp()
    try {
      const p = a.page
      const id = await completedReview(a)
      const cdp = await p.context().newCDPSession(p)
      const held: string[] = []
      let reads = 0, exports = 0, folders = 0
      cdp.on('Fetch.requestPaused', (e: any) => {
        if (e.request.method === 'OPTIONS') { void cdp.send('Fetch.continueRequest', { requestId: e.requestId }); return }
        const command = new URL(e.request.url).pathname.slice(1)
        if (command === 'run_export') exports++
        if (command === 'open_folder') folders++
        if (command === 'run_get') {
          reads++
          if (mode !== 'loaded') { held.push(e.requestId); return }
        }
        void cdp.send('Fetch.continueRequest', { requestId: e.requestId })
      })
      await cdp.send('Fetch.enable', { patterns: ['run_get', 'run_export', 'open_folder'].map(cmd => ({ urlPattern: `http://ipc.localhost/${cmd}*`, requestStage: 'Request' })) })
      // Reload clears the prior snapshot; connect() publishes the persisted summary first.
      await p.goto(`${BASE}/review/${id}/export`)
      await expect(p.locator('[data-screen="S10"]')).toContainText('R85 real review')
      await expect.poll(() => reads).toBeGreaterThan(0)
      if (mode !== 'loaded') {
        for (const button of ['do-export', 'open-folder', 'export-partial']) {
          await expect(p.getByTestId(button)).toBeDisabled()
          // Native DOM click also respects disabled: no fabricated success or IPC.
          await p.getByTestId(button).evaluate((el: HTMLButtonElement) => el.click())
        }
        await expect(p.getByTestId('export-result')).toHaveCount(0)
        await expect(p.getByTestId('sample-export-unavailable')).toHaveCount(0)
        if (mode === 'failed') {
          // runs_list has already succeeded; now the actual backend read fails.
          const state = join(a.workspace, 'runs', id, 'run_state.json')
          renameSync(state, `${state}.r85-backup`)
          for (const requestId of held.splice(0)) await cdp.send('Fetch.continueRequest', { requestId })
          await expect(p.getByTestId('export-load-error')).toBeVisible()
          await expect(p.getByTestId('export-load-error')).not.toBeEmpty()
        } else {
          await expect(p.locator('[data-screen="S10"]')).toContainText('Loading')
        }
        await expect(p.getByTestId('do-export')).toBeDisabled()
        await expect(p.getByTestId('export-result')).toHaveCount(0)
        expect({ exports, folders }).toEqual({ exports: 0, folders: 0 })
      } else {
        await expect(p.getByTestId('do-export')).toBeEnabled()
        const final = join(a.workspace, 'runs', id, 'output', 'final', 'final_document.md')
        expect(existsSync(final)).toBe(true)
        renameSync(final, `${final}.r85-backup`)
        await p.getByTestId('do-export').click()
        await expect(p.getByTestId('export-result')).toContainText('Exported to')
        expect(exports).toBe(1)
        expect(existsSync(final), 'real export recreated the missing file').toBe(true)
        const path = await p.getByTestId('run-path').innerText()
        for (const file of ['final/final_document.docx', 'final/final_review.html', 'audit/run_manifest.json']) expect(existsSync(join(path, file)), file).toBe(true)
      }
      console.log(JSON.stringify({ mode, reads, exports, folders }))
      await cdp.detach()
    } finally { await a.stop() }
  })
}
