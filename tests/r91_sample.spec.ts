// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { readFileSync } from 'node:fs'
import { FAST, expect, setupDone, test } from './fixtures'
import { markdownBlocks } from '../src/lib/runView'

test('served Northwind document matches its source and renders without licence metadata', async ({ request }) => {
  const response = await request.get('/samples/business_case_northwind.md')
  expect(response.ok()).toBeTruthy()
  const text = await response.text()
  expect(text.replace(/\r\n/g, '\n')).toBe(readFileSync('samples/business_case_northwind.md', 'utf8').replace(/\r\n/g, '\n'))
  expect(text).not.toContain('SPDX')
  expect(markdownBlocks(text)[0]).toEqual({ kind: 'h', text: 'Northwind Logistics: Midwest Hub Expansion — Business Case' })
  expect(markdownBlocks(text).map(block => block.text).join('\n')).not.toContain('Copyright')
})

test('sample review document displays no SPDX text', async ({ page }) => {
  await setupDone(page)
  await page.goto(`/review/sample?${FAST}`)
  await page.getByTestId('tab-original').click()
  const document = page.getByTestId('doc-original')
  await expect(document).toContainText('Northwind Logistics: Midwest Hub Expansion')
  await expect(document).not.toContainText('SPDX')
  await expect(document).not.toContainText('Copyright')
})
