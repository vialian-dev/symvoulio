// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Desktop E2E: the real Tauri debug app (real IPC, real engine, the fake provider CLI) driven over WebView2 CDP.
// Windows only. Run: npx playwright test -c playwright.desktop.config.ts [file]
import { defineConfig } from '@playwright/test'
export default defineConfig({
  testDir: 'tests/desktop',
  testMatch: /.*\.dspec\.ts/,
  // Bundled frontends have no Vite watcher or HMR; retain these checks in the dev run.
  testIgnore: process.env.SYMVOULIO_E2E_BASE && !process.env.SYMVOULIO_DEV_BASE
    ? /ct_r07b_harness|ct_r59_design_hmr|ct_r59_watch_root|ct_r36_watch/
    : /ct_r07b_harness/,
  timeout: 300_000,
  workers: 1,
  reporter: [['list']],
  globalSetup: './tests/desktop/global-setup.ts',
  // Each run keeps its own output dir (traces and logs of earlier runs are not wiped), named by UTC start time.
  outputDir: `test-results/desktop-${new Date().toISOString().replace(/[:.]/g, '-')}`,
})
