// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Omit private review metadata from comments in the public copy only.
import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const root = process.argv[2]
if (!root) throw new Error('usage: sanitize-comments.mjs <export directory>')
const files = [
  'tests/ct_v02_field.check.ts',
  'tests/ct_v02_r76_78.check.ts',
  'tests/desktop/ct_v02_field.dspec.ts',
  'tests/desktop/ct_v02_r76_78.dspec.ts',
].filter(file => existsSync(join(root, file)))
if (files.length) {
  const { createJiti } = await import('jiti')
  const compiler = createJiti(import.meta.url, { fsCache: false })
  const parse = (filename, source) => {
    let comments = []
    const code = compiler.transform({ source, filename, ts: true, babel: {
      comments: false, compact: true,
      plugins: [() => ({ visitor: { Program(path) { comments = path.hub.file.ast.comments } } })],
    } })
    return { code, comments }
  }
  for (const file of files) {
    const path = join(root, file)
    const text = readFileSync(path, 'utf8')
    const parsed = parse(path, text)
    const ranges = new Map()
    for (const comment of parsed.comments) {
      if (comment.type !== 'CommentLine') continue
      const lineStart = text.lastIndexOf('\n', comment.start - 1) + 1
      if (text.slice(lineStart, comment.start).trim()) continue
      if (/[D]-SY-\d|[a]stra\b/i.test(comment.value)) ranges.set(comment.start, comment.end)
    }
    let clean = text
    for (const [start, end] of [...ranges].sort((a, b) => b[0] - a[0])) clean = clean.slice(0, start) + clean.slice(end)
    if (parsed.code !== parse(path, clean).code) throw new Error(`sanitizer changed executable code: ${file}`)
    writeFileSync(path, clean)
    console.log(`public comments: ${file}: ${ranges.size} metadata comments omitted; executable code unchanged`)
  }
}
