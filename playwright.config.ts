// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { defineConfig } from '@playwright/test'

// Gates of the clickable prototype. Local: Playwright starts `vite preview` (port 4173) and stops it itself.
// Stand: BASE_URL=<url of a deployed build> npx playwright test --workers=1 (no local server).
// Memory (lesson L53): run with NODE_OPTIONS=--max-old-space-size=4096 and --workers=1 (other agents test on this machine).
const stand = process.env.BASE_URL
export default defineConfig({
  testDir: 'tests',
  timeout: 60_000,
  workers: 1,
  reporter: [['list'], ['json', { outputFile: 'test-results/gates.json' }]],
  use: { baseURL: stand ?? 'http://localhost:4173', viewport: { width: 1440, height: 900 } },
  webServer: stand ? undefined : { command: 'npx vite preview --port 4173 --strictPort', url: 'http://localhost:4173', reuseExistingServer: false, timeout: 60_000 },
})
