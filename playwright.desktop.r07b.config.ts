// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// R07b: the harness must not accept the WebView2 error page as «app ready». These tests control the dev server
// themselves (late start, held requests), so there is NO globalSetup. Run: npx playwright test -c playwright.desktop.r07b.config.ts
import { defineConfig } from '@playwright/test'
export default defineConfig({
  testDir: 'tests/desktop',
  testMatch: /ct_r07b_harness\.dspec\.ts/,
  timeout: 300_000,
  workers: 1,
  reporter: [['list']],
  outputDir: `test-results/desktop-r07b-${new Date().toISOString().replace(/[:.]/g, '-')}`,
})
