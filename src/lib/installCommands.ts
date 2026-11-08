// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
export type InstallOS = 'windows' | 'macos' | 'linux'
export interface InstallInput { provider: 'claude' | 'codex'; os: InstallOS; shell: string; installed?: boolean; node?: 'present' | 'missing' | 'old'; nativeAvailable?: boolean }
// Official sources, accessed 2026-11-01 and re-read for macOS 2026-11-05 (commands unchanged):
// https://code.claude.com/docs/en/setup and https://github.com/openai/codex
// See .hardening/v02/OFFICIAL_INSTALL.md. PS7 intentionally launches powershell for Codex.
const COMMANDS = {
  claude: { windows: 'irm https://claude.ai/install.ps1 | iex', unix: 'curl -fsSL https://claude.ai/install.sh | bash', npm: 'npm install -g @anthropic-ai/claude-code' },
  codex: { windows: 'powershell -ExecutionPolicy ByPass -c "irm https://chatgpt.com/codex/install.ps1 | iex"', unix: 'curl -fsSL https://chatgpt.com/codex/install.sh | sh', npm: 'npm install -g @openai/codex' },
}
// macOS: zsh, or bash where zsh is missing (VAL.5); both run the same POSIX install line.
export function installPlan(i: InstallInput): { ok: true; command: string; method: 'native' | 'npm' } | { ok: false; error: string; link?: string } {
  if (i.installed) return { ok: false, error: 'Already installed' }
  const shell = i.shell.split(/[\\/]/).pop()?.toLowerCase().replace(/\.exe$/, '')
  if (!(i.os === 'windows' ? shell === 'pwsh' || shell === 'powershell' : i.os === 'macos' ? shell === 'zsh' || shell === 'bash' : i.os === 'linux' && shell === 'bash')) return { ok: false, error: 'Unsupported install shell' }
  if (i.nativeAvailable === false) return i.node === 'present' ? { ok: true, command: COMMANDS[i.provider].npm, method: 'npm' } : { ok: false, error: 'Node.js is required', link: 'https://nodejs.org/en/download' }
  return { ok: true, command: COMMANDS[i.provider][i.os === 'windows' ? 'windows' : 'unix'], method: 'native' }
}
