// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// R51 (canned backend; the Rust engine has the same rule in crates/symvoulio-core/src/engine.rs): Return -> Pause -> Resume
// must match the desktop engine, which keeps `returned_until` through Pause. The Return allows exactly ONE more round: the run
// is stopped mid round 2 (done 1), returned (limit 2), paused and resumed, and must END after round 2, never start round 3.
import { expect, setupDone, test } from './fixtures'

test('R51: Return -> Pause -> Resume ends after the one extra round (Round 2 of 5, not Running), no round 3', async ({ page }) => {
  await setupDone(page)
  await page.goto('/new?speed=1')
  await page.getByTestId('s6-name').fill('R51')
  await page.getByTestId('s6-objective').fill('Improve this document for executive review without adding unsupported claims.')
  await page.getByTestId('templates').click()
  await page.getByTestId('template-business_case').click()
  await page.getByTestId('approve-criteria').click()
  await page.getByTestId('primary-input').setInputFiles({ name: 'case.md', mimeType: 'text/markdown', buffer: Buffer.from('# Case') })
  await page.getByTestId('start').click()
  await expect(page.locator('[data-screen="S7"]')).toBeVisible()
  const strip = page.getByTestId('status-strip')
  await expect(page.getByTestId('term-codex').locator('.xterm-rows')).toContainText('Review 2 started', { timeout: 30_000 })
  await page.getByTestId('stop').click()
  await expect(strip).toContainText('Stopped')
  // Return resumes these reviewer steps. Keep time held while making UI decisions,
  // so the run cannot finish and disable Pause before the click is actionable.
  // At speed=1 even the final remaining tick leaves 850 ms for S7's 350 ms entrance.
  await page.clock.install()
  await page.clock.pauseAt(new Date(Date.now() + 1000))
  await page.getByTestId('human-decision').click()
  await page.clock.runFor(400) // finish S9's entrance before interacting
  await page.getByTestId('return').click()
  await page.getByTestId('decider-name').fill('Sam Ortega')
  await page.getByTestId('confirm').click()
  await page.clock.runFor(400) // finish S7's entrance, leaving reviewer steps to pause
  await expect(page.locator('[data-screen="S7"]')).toBeVisible()
  await expect(strip).toContainText('Round 2 of 5')
  await expect(page.getByTestId('pause')).toBeEnabled()
  await page.getByTestId('pause').click()
  await expect(strip).toContainText('Paused')
  await page.getByTestId('resume').click()
  await page.clock.runFor(5000)
  await expect(strip, 'the run ends after the extra round, as on the desktop').toContainText(/Completed|Stopped/, { timeout: 30_000 })
  await expect(strip).toContainText('Round 2 of 5')
  // Nothing continues afterwards: still ended in round 2, and no round 3 was started.
  await page.clock.runFor(3000)
  await expect(strip).toContainText(/Completed|Stopped/)
  await expect(strip).toContainText('Round 2 of 5')
  await expect(page.getByTestId('round-3')).toHaveCount(0)
})
