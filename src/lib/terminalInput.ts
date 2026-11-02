// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
/** Clipboard text is never raw terminal input. */
export function pasteDecision(text: string): 'paste' | 'refuse' {
  return /[\p{Cc}]/u.test(text) ? 'refuse' : 'paste'
}
