// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
const LIMIT = 200
export const aliasesKey = (workspace: string) => `symvoulio.runAliases.v2:${encodeURIComponent(workspace)}`
export function loadAliases(workspace: string): [string, string][] {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(aliasesKey(workspace)) ?? '[]')
    return Array.isArray(value) ? value.filter((pair): pair is [string, string] => Array.isArray(pair) && pair.length === 2 && pair.every(id => typeof id === 'string' && /^[A-Za-z0-9-]+$/.test(id))).slice(-LIMIT) : []
  } catch { return [] }
}
export function persistAliases(workspace: string, ids: Map<string, string>) {
  try { localStorage.setItem(aliasesKey(workspace), JSON.stringify([...ids].slice(-LIMIT))) } catch { /* Browser storage is optional. */ }
}
export function pruneAliases(ids: Map<string, string>, runs: Set<string>, newlyBound: Set<string>) {
  for (const [alias, id] of ids) if (!runs.has(id) && !newlyBound.has(id)) ids.delete(alias)
}
