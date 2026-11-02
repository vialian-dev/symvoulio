// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { existsSync } from 'node:fs'
import { relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

// The brand mark (concept 5) is used once its files are in public/brand/; until then the frame keeps
// the empty mark slot. Decided at build time, so a missing file never shows as a broken image.
const BRAND = existsSync('public/brand/symvoulio_mark_deep_64.png') && existsSync('public/brand/symvoulio_mark_light_64.png')

// Keep the root traversable, but watch only frontend inputs. Vite passes whole paths to ignored.
const frontendRoot = fileURLToPath(new URL('.', import.meta.url))
const ignoreNonFrontend = (path: string) => {
  const name = relative(frontendRoot, resolve(path)).replace(/\\/g, '/')
  return !(name === '' || /^(src|public)(\/|$)/.test(name) ||
    name === 'packages' || /^packages\/design-system(\/|$)/.test(name) ||
    name === 'index.html' || /^(vite|tailwind|postcss)\.config\.[^/]+$/.test(name) ||
    /^tsconfig(?:\.[^/]+)?\.json$/.test(name))
}

// The same build serves the Tauri window (src-tauri/tauri.conf.json frontendDist = ../dist) and the browser stand.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  define: { __BRAND_MARK__: JSON.stringify(BRAND) },
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    watch: {
      ignored: ignoreNonFrontend,
    },
  },
  build: { target: 'es2022', chunkSizeWarningLimit: 1500 },
})
