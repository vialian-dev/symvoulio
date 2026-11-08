// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Symvoulio 0.3.1 fixes (the F1-F8 fix order), browser stand with the canned provider:
// F2 «Sign in» runs the login in the panel and the status turns connected by itself; F4 the panel is clean afterwards
// and after the wizard; F5 the help for steps 1-3 is at the bottom of the left rail and follows the state; no picture
// placeholders; F8 the Reviews date is whole.
import { FAST as SPEED, expect, setupDone, term, test } from './fixtures'

// The browser stand runs as Windows; `os=macos` shows the macOS screens (Windows keeps the 0.3.0 flow).
const FAST = `${SPEED}&os=macos`

test.describe('0.3.1', () => {
  test('F2 + F4: «Sign in» runs the login, connects by itself and leaves a clean panel (Claude Code, then Codex)', async ({ page }) => {
    await page.goto(`/setup/claude?${FAST}`)
    await expect(page.getByTestId('connected')).toHaveCount(0)
    await page.getByTestId('sign-in').click()
    // No Enter, no Check connection by the user: the status turns connected on its own.
    await expect(page.getByTestId('connected')).toContainText('Claude Code connected')
    await expect(page.getByTestId('sign-in')).toBeDisabled()
    // F4: the login transcript («Login successful») is gone; the panel shows a clean prompt.
    await expect(term(page, 'claude')).not.toContainText('Login successful')
    await expect(term(page, 'claude')).not.toContainText('claude auth login')
    await expect(page.getByTestId('signing-in')).toHaveCount(0)
    await page.getByTestId('next').click()
    await expect(page.locator('[data-screen="S3"]')).toBeVisible()
    await page.getByTestId('sign-in').click()
    await expect(page.getByTestId('connected')).toContainText('Codex connected')
    await expect(term(page, 'codex')).not.toContainText('Successfully logged in')
  })

  test('F4: finishing the wizard clears what the setup left in the panels', async ({ page }) => {
    await page.goto(`/setup/claude?${FAST}`)
    await page.getByTestId('paste-login').click()
    await expect(term(page, 'claude')).toContainText('claude auth login')
    await page.goto(`/setup/policy?${FAST}`)
    await page.getByTestId('finish').click()
    await expect(page).toHaveURL(/\/reviews/)
    await expect(term(page, 'claude')).not.toContainText('claude auth login')
  })

  test('F5: «What to do now» at the bottom of the left rail on steps 1-3, step-specific and following the state', async ({ page }) => {
    const help = page.getByTestId('setup-help')
    const expected: [string, string, RegExp][] = [
      ['/setup/system', 'help.system.signIn', /Press «Continue»/],
      ['/setup/claude', 'help.connect.signIn', /Press «Sign in»\. Claude Code starts its sign-in/],
      ['/setup/codex', 'help.connect.signIn', /Press «Sign in»\. Codex starts its sign-in.*ChatGPT account/],
    ]
    for (const [path, key, text] of expected) {
      await page.goto(`${path}?${FAST}`)
      await expect(help).toBeVisible()
      await expect(help).toHaveAttribute('data-help', key)
      await expect(page.getByTestId('setup-help-text')).toHaveText(text)
      // Inside the left rail, below its links, above Settings.
      const nav = (await page.getByTestId('nav').boundingBox())!
      const box = (await help.boundingBox())!
      const settings = (await page.getByTestId('nav-settings').boundingBox())!
      const newReview = (await page.getByTestId('nav-new').boundingBox())!
      expect(box.x).toBeGreaterThanOrEqual(nav.x)
      expect(box.x + box.width).toBeLessThanOrEqual(nav.x + nav.width + 0.5)
      expect(box.y).toBeGreaterThan(newReview.y + newReview.height)
      expect(box.y + box.height).toBeLessThanOrEqual(settings.y + 0.5)
      expect(box.y).toBeGreaterThan(nav.y + nav.height / 2)
      // No picture placeholders anywhere on the step.
      await expect(page.getByTestId('shot-slot')).toHaveCount(0)
      await expect(page.getByText(/^Screenshot:/)).toHaveCount(0)
    }
    // It follows the state: after the sign-in the Codex step says to go on.
    await page.getByTestId('sign-in').click()
    await expect(help).toHaveAttribute('data-help', 'help.connect.done')
    await expect(page.getByTestId('setup-help-text')).toHaveText('Codex is connected. Press «Next».')
    // Only on the first three steps.
    for (const path of ['/setup/roles', '/setup/policy']) {
      await page.goto(`${path}?${FAST}`)
      await expect(help).toHaveCount(0)
    }
    // After setup (an unfinished setup sends /reviews back to the wizard), Reviews and Settings show no help.
    await setupDone(page)
    for (const path of ['/reviews', '/settings']) {
      await page.goto(`${path}?${FAST}`)
      await expect(page.getByTestId('nav-settings')).toBeVisible()
      await expect(help).toHaveCount(0)
    }
  })

  test('Windows keeps the 0.3.0 setup flow: no «Sign in», no rail help, the five picture-slot steps', async ({ page }) => {
    await page.goto(`/setup/claude?${SPEED}`)
    await expect(page.getByTestId('guide-step-5')).toBeVisible()
    await expect(page.getByTestId('shot-slot')).toHaveCount(5)
    await expect(page.getByTestId('sign-in')).toHaveCount(0)
    await expect(page.getByTestId('setup-help')).toHaveCount(0)
    expect((await page.getByTestId('nav').boundingBox())!.width).toBeCloseTo(84, 0)
    await expect(page.getByText('Symvoulio never types Enter for you.', { exact: false })).toBeVisible()
  })

  test('F2: «Cancel» stops the wait; a stale attempt never connects; «Sign in» again connects', async ({ page }) => {
    await page.goto('/setup/claude?os=macos')
    await page.getByTestId('sign-in').click()
    await expect(page.getByTestId('signing-in')).toBeVisible()
    await page.getByTestId('sign-in-cancel').click()
    await expect(page.getByTestId('signing-in')).toHaveCount(0)
    await expect(page.getByTestId('setup-help')).toHaveAttribute('data-help', 'help.connect.signIn')
    await page.waitForTimeout(1500)
    await expect(page.getByTestId('connected')).toHaveCount(0)
    await page.getByTestId('sign-in').click()
    await expect(page.getByTestId('connected')).toContainText('Claude Code connected')
  })

  test('F5: with «API key» the rail help says to paste and run the command, not «Sign in»', async ({ page }) => {
    await page.goto(`/setup/codex?${FAST}`)
    await page.getByTestId('method-api-key').click()
    await expect(page.getByTestId('sign-in')).toHaveCount(0)
    await expect(page.getByTestId('setup-help')).toHaveAttribute('data-help', 'help.connect.apiKey')
    await expect(page.getByTestId('setup-help-text')).toContainText('Press «Paste command», click into the Codex panel')
    await page.getByTestId('method-subscription').click()
    await expect(page.getByTestId('setup-help')).toHaveAttribute('data-help', 'help.connect.signIn')
  })

  test('F8: the Reviews date reads «Mon D, HH:MM», whole, at the smallest window', async ({ page }) => {
    await setupDone(page)
    await page.setViewportSize({ width: 1280, height: 800 })
    await page.goto(`/reviews?${FAST}`)
    const dates = page.getByTestId('run-date')
    await expect(dates.first()).toBeVisible()
    for (const d of await dates.all()) {
      expect(await d.innerText()).toMatch(/^[A-Z][a-z]{2} \d{1,2}, \d{2}:\d{2}$/)
      expect(await d.evaluate((e) => e.scrollWidth <= e.clientWidth)).toBe(true)
    }
  })
})
