// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { expect, setupDone, test } from './fixtures'

test('setup and About disclose external document sharing consistently', async ({ page }) => {
  await setupDone(page)
  let setupText = ''
  for (const path of ['/setup/system', '/about']) {
    await page.goto(path)
    const disclosure = page.getByTestId('provider-disclosure')
    await expect(disclosure).toBeVisible()
    await expect(disclosure).toContainText('documents, objective, criteria and supporting files are sent')
    await expect(disclosure).toContainText('Anthropic via Claude Code and OpenAI via Codex')
    await expect(disclosure).toContainText("your own CLI accounts and are subject to the providers' terms")
    if (!setupText) setupText = await disclosure.innerText()
    else await expect(disclosure).toHaveText(setupText)
  }
})
