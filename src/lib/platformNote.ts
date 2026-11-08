// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
/** Which first-run note S13 shows: macOS (Gatekeeper, Open Anyway) on a Mac, otherwise the Windows SmartScreen note.
 *  `os` is `app_info().os` (Rust `std::env::consts::OS`); the browser stand has no app_info and keeps Windows. */
export function firstRunNote(os?: string): 'macos' | 'windows' {
  return os === 'macos' ? 'macos' : 'windows'
}

/** F7: the Settings note on panel shells for this OS: zsh/bash on a Mac, PowerShell elsewhere (unchanged on Windows). */
export function shellHintKey(os?: string): 's12.shellHintMac' | 's12.shellHint' {
  return os === 'macos' ? 's12.shellHintMac' : 's12.shellHint'
}
