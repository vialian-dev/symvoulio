// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// The OS the app runs on, for the parts of 0.3.1 that are macOS-only. Windows keeps the 0.3.0 setup flow unchanged
// (review r1): «Sign in» run by the app (F2), the panel resets (F4) and the rail help (F5) apply on macOS and Linux.
import { useSyncExternalStore } from 'react'
import { installEnvironment } from './backend'

export type Os = 'windows' | 'macos' | 'linux'
let os: Os | undefined
let started = false
const listeners = new Set<() => void>()
function start() {
  if (started) return
  started = true
  void installEnvironment().then((e) => { os = e.os; listeners.forEach((l) => l()) }).catch(() => { /* unknown OS: Windows flow */ })
}
export function usePlatform(): Os | undefined {
  start()
  return useSyncExternalStore((l) => { listeners.add(l); return () => { listeners.delete(l) } }, () => os)
}
export function currentOs(): Os | undefined { start(); return os }
/** The 0.3.1 setup flow (F2, F4, F5): macOS and Linux. Windows, and an OS not known yet, keep the 0.3.0 flow. */
export const appSignIn = (value: Os | undefined) => value === 'macos' || value === 'linux'
