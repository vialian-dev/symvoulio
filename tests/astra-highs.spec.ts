// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { expect, test } from '@playwright/test'
import { diffDocs, diffWords } from '../src/lib/diff'
import { fileBytes, sampleFile, revisionFile } from '../src/services/files'

test('deleted blocks survive block alignment', () => {
 const rows = diffDocs([{ kind: 'p', text: 'Keep' }, { kind: 'p', text: 'Deleted risk' }], [{ kind: 'p', text: 'Keep' }])
 expect(rows[1].parts).toEqual([{ kind: 'del', text: 'Deleted risk' }])
})
test('50,000 words use a bounded replacement', () => {
 const start = performance.now()
 expect(diffWords('old '.repeat(50000), 'new '.repeat(50000)).map(p => p.kind)).toEqual(['del', 'add'])
 expect(performance.now() - start).toBeLessThan(2000)
})
test('served sample and saved revision have registered bytes', async ({ request }) => {
 const response = await request.get('/samples/business_case_northwind.md')
 expect(response.ok()).toBeTruthy()
 const text = await response.text()
 const chip = await sampleFile(async () => new Response(text))
 expect(new TextDecoder().decode(new Uint8Array(await fileBytes(chip)))).toBe(text)
 const revision = revisionFile(3, '# Saved revision')
 expect(new TextDecoder().decode(new Uint8Array(await fileBytes(revision)))).toBe('# Saved revision')
})
