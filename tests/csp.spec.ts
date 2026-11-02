// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { test, expect } from '@playwright/test'
import { readFileSync } from 'node:fs'
const csp = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8')).app.security.csp as string
test('production CSP allows bundled UI; strict styles demonstrate inline requirement', async ({ page }) => {
  const html = readFileSync('dist/index.html', 'utf8')
  const scripts = [...html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/g)]
  expect(scripts.length).toBeGreaterThan(0)
  for (const script of scripts) { expect(script[1]).toMatch(/src=/); expect(script[2].trim()).toBe('') }
  let policy = csp.replace(" 'unsafe-inline'", '')
  await page.route('**/*', async route => {
    const response = await route.fetch()
    await route.fulfill({ response, headers: { ...response.headers(), ...(route.request().resourceType() === 'document' ? { 'content-security-policy': policy } : {}) } })
  })
  await page.addInitScript(() => {
    const violations: string[] = []
    Object.assign(window, { cspViolations: violations })
    document.addEventListener('securitypolicyviolation', e => violations.push(e.effectiveDirective))
  })
  await page.goto('/')
  await expect(page.locator('.xterm').first()).toBeVisible()
  await expect.poll(() => page.evaluate(() => (window as unknown as { cspViolations: string[] }).cspViolations.filter(v => v.startsWith('style-src')).length)).toBeGreaterThan(0)
  const blocked = await page.evaluate(() => (window as unknown as { cspViolations: string[] }).cspViolations)
  console.log(`CSP measurement: without unsafe-inline: ${blocked.filter(v => v.startsWith('style-src')).length} style violations`)
  policy = csp
  await page.reload()
  await expect(page.locator('.xterm').first()).toBeVisible()
  await page.waitForTimeout(500)
  expect(await page.evaluate(() => (window as unknown as { cspViolations: string[] }).cspViolations)).toEqual([])
  console.log(`CSP measurement: configured policy: 0 violations; ${scripts.length} external scripts, 0 inline scripts`)
})
