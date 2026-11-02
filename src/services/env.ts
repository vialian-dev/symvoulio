// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
/** True inside the Tauri desktop app; false in the browser (the stand and the Playwright gates run canned). */
export const isDesktop = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
