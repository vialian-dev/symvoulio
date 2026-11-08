// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
/** One-line date of a run in UTC: «Oct 30, 19:56». F8: built from its parts, never from Intl's own date-time pattern,
 *  which differs by engine: WebKit on macOS joins date and time with « at » («Nov 7 at 23:04»), and the wider text was
 *  cut off in the Reviews list. */
export function shortDate(iso: string): string {
  const parts = new Intl.DateTimeFormat('en', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', hourCycle: 'h23', timeZone: 'UTC' }).formatToParts(new Date(iso))
  const part = (type: Intl.DateTimeFormatPartTypes) => parts.find((p) => p.type === type)?.value ?? ''
  return `${part('month')} ${part('day')}, ${part('hour')}:${part('minute')}`
}
