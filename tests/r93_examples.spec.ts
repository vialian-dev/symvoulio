// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { expect, setupDone, test } from './fixtures'

test('fixed examples are disclosed for different objectives and remain editable', async ({ page }) => {
  await setupDone(page)
  for (const objective of ['Improve this document for executive review.', 'Check cost arithmetic only.']) {
    await page.goto('/new')
    await page.getByTestId('s6-objective').fill(objective)
    await expect(page.getByTestId('suggest')).toHaveText('Suggest examples')
    await expect(page.getByTestId('suggest-note')).toContainText('No AI or provider is called.')
    await page.getByTestId('suggest').click()
    await expect(page.getByTestId('criteria').locator('input')).toHaveCount(2)
    await expect(page.getByTestId('criterion-0')).toHaveValue('Every forecast names its source and year.')
    await page.getByTestId('suggest').click()
    await expect(page.getByTestId('criteria').locator('input')).toHaveCount(2)
    await page.getByTestId('approve-criteria').click()
    await page.getByTestId('criterion-0').fill('Check the arithmetic.')
    await expect(page.getByTestId('approve-criteria')).toBeVisible()
  }
})
