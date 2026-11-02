// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import type { Provider } from '../data/sample'
export function apiKeyLogin(provider: Provider, shell: string): string {
  if (provider === 'claude') return 'claude auth login --console'
  return /(?:^|[\\/])(?:pwsh|powershell)(?:\.exe)?$/i.test(shell)
    ? '$env:OPENAI_API_KEY | codex login --with-api-key'
    : 'printenv OPENAI_API_KEY | codex login --with-api-key'
}
