// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// (canned backend; the Rust engine has the same rule and its own tests in crates/symvoulio-core):
// «Return for another round» is disabled at the maximum of 5 rounds, whether the last round completed or the run was
// stopped in the middle of it; a run stopped mid-round keeps its written revision and Return finishes that round.
import { expect, setupDone, test } from './fixtures'
import type { Page } from '@playwright/test'

const HINT = 'Maximum rounds reached — use Restart from revision on Reviews'

/** S6 → Start with the sample criteria and a small .md; `minFive` makes the canned run go to round 5. */
async function startRun(page: Page, speed: number, minFive: boolean, max?: number) {
  await setupDone(page)
  await page.goto(`/new?speed=${speed}`)
  await page.getByTestId('s6-name').fill('Rule check')
  await page.getByTestId('s6-objective').fill('Improve this document for executive review without adding unsupported claims.')
  await page.getByTestId('templates').click()
  await page.getByTestId('template-business_case').click()
  await page.getByTestId('approve-criteria').click()
  await page.getByTestId('primary-input').setInputFiles({ name: 'case.md', mimeType: 'text/markdown', buffer: Buffer.from('# Case') })
  if (minFive || max) {
    await page.getByTestId('policy-toggle').click()
    if (minFive) await page.getByTestId('s6pol-min-5').click()
    if (max) await page.getByTestId(`s6pol-max-${max}`).click()
  }
  await page.getByTestId('start').click()
  await expect(page.locator('[data-screen="S7"]')).toBeVisible()
}

async function returnButton(page: Page) {
  await page.getByTestId('human-decision').click()
  await expect(page.getByTestId('not-approval')).toBeVisible()
  return page.getByTestId('return')
}

test.describe('Return at the maximum of rounds', () => {
  test('a run that completed round 5 of 5 cannot return', async ({ page }) => {
    await startRun(page, 50, true)
    await expect(page.getByTestId('status-strip')).toContainText('Round 5 of 5', { timeout: 30_000 })
    await expect(page.getByTestId('human-decision')).toBeEnabled({ timeout: 30_000 })
    const back = await returnButton(page)
    await expect(back).toBeDisabled()
    await expect(back).toHaveAttribute('title', HINT)
  })

  test('a run stopped in the middle of round 5 of 5 cannot return', async ({ page }) => {
    await startRun(page, 6, true)
    await expect(page.getByTestId('status-strip')).toContainText('Round 5 of 5', { timeout: 60_000 })
    await page.waitForTimeout(600) // a step or more into round 5: the author's revision 5 is being written
    await page.getByTestId('stop').click()
    await expect(page.getByTestId('status-strip')).toContainText('Round 5 of 5')
    await expect(page.getByTestId('status-strip')).toContainText('Stopped')
    const back = await returnButton(page)
    await expect(back).toBeDisabled()
    await expect(back).toHaveAttribute('title', HINT)
  })

  test('a run stopped during the review of round 2 keeps revision 2, and Return finishes that review', async ({ page }) => {
    await startRun(page, 4, false)
    await expect(page.getByTestId('term-codex').locator('.xterm-rows')).toContainText('Review 2 started', { timeout: 30_000 })
    await page.getByTestId('stop').click()
    await expect(page.getByTestId('status-strip')).toContainText('Round 2 of 5')
    await expect(page.getByTestId('doc-current')).toContainText('$4.6M') // revision 2 (revision 1 still says $4.2M)
    const back = await returnButton(page)
    await expect(back).toBeEnabled()
    await back.click()
    await page.getByTestId('decider-name').fill('Dana Whitfield')
    await page.getByTestId('confirm').click()
    await expect(page.locator('[data-screen="S7"]')).toBeVisible()
    // The same round goes on with the revision already written: no replay of the author's step.
    await expect(page.getByTestId('status-strip')).toContainText('Round 2 of 5')
    await expect(page.getByTestId('doc-current')).toContainText('$4.6M')
  })

  // the run's OWN maximum, not a fixed 5.
  for (const max of [1, 3]) {
    test(`a run that completed its last round at max ${max} cannot return`, async ({ page }) => {
      await startRun(page, 50, false, max)
      await expect(page.getByTestId('status-strip')).toContainText(`Round ${max} of ${max}`, { timeout: 30_000 })
      await expect(page.getByTestId('human-decision')).toBeEnabled({ timeout: 30_000 })
      const back = await returnButton(page)
      await expect(back).toBeDisabled()
      await expect(back).toHaveAttribute('title', HINT)
    })
  }
})
