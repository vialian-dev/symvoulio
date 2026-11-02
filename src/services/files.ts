// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import type { FileChip } from '../state/store'
const files = new WeakMap<FileChip, Blob>()
/** Register the selected file before passing its chip to startRun. */
export function registerFile(chip: FileChip, file: Blob) { files.set(chip, file); return chip }
export async function fileBytes(chip: FileChip): Promise<number[]> {
  const file = files.get(chip)
  if (!file) throw new Error('Selected file bytes have not been registered')
  return Array.from(new Uint8Array(await file.arrayBuffer()))
}
export function revisionFile(revision: number, text: string): FileChip {
 const blob = new Blob([text], { type: 'text/markdown' })
 return registerFile({ name: `revision_${String(revision).padStart(2, '0')}.md`, size: blob.size, type: 'md' }, blob)
}
export async function sampleFile(load: typeof fetch = fetch): Promise<FileChip> {
 const response = await load('/samples/business_case_northwind.md')
 if (!response.ok) throw new Error(`Sample could not be loaded: ${response.status}`)
 const blob = await response.blob()
 return registerFile({ name: 'business_case_northwind.md', size: blob.size, type: 'md' }, blob)
}

const tokens = new WeakMap<FileChip, string>()
export function registerStaged(name: string, token: string): FileChip {
 const chip = { name, size: 0, type: name.split('.').pop() ?? '' }
 tokens.set(chip, token)
 return chip
}
export const stagedToken = (chip: FileChip) => tokens.get(chip)
export const fileAvailable = (chip: FileChip) => files.has(chip) || tokens.has(chip)
export const forgetStaged = (chip: FileChip) => tokens.delete(chip)
