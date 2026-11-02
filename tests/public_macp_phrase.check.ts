// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// The public-export guard allows the owner-approved product phrase verbatim in README.md and RELEASE_NOTES.md only.
// Run: npx jiti tests/public_macp_phrase.check.ts
import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

const root = resolve(__dirname, '..')
const checkSh = join(root, 'tools', 'public-export', 'check.sh').replace(/\\/g, '/')
// Built from parts so this file itself passes the public-export check.
const NAME = ['Mensura', 'bilitas'].join('')
const ACRO = ['MA', 'CP'].join('')
const PHRASE = `${NAME} Automatic Coding Pipeline (${ACRO})`

function findBash(): string {
  for (const c of [process.env.CT_BASH, 'C:/Program Files/Git/bin/bash.exe', 'bash'].filter(Boolean) as string[]) {
    const r = spawnSync(c, ['-c', 'echo ok'], { encoding: 'utf8' })
    if (r.status === 0 && r.stdout.trim() === 'ok') return c
  }
  throw new Error('no bash found (set CT_BASH)')
}
const bash = findBash()

let failed = 0
function check(name: string, fn: () => void) {
  try { fn(); console.log(`PASS ${name}`) } catch (e) { failed++; console.log(`FAIL ${name}\n  ${(e as Error).message}`) }
}

function run(files: Record<string, string>) {
  const dir = mkdtempSync(join(tmpdir(), 'macp-tree-'))
  try {
    for (const [name, text] of Object.entries(files)) {
      mkdirSync(join(dir, name, '..'), { recursive: true })
      writeFileSync(join(dir, name), text)
    }
    const r = spawnSync(bash, [checkSh, dir.replace(/\\/g, '/')], { encoding: 'utf8' })
    return { status: r.status, out: r.stdout + r.stderr }
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
}

check('exact phrase in README.md passes', () => {
  const r = run({ 'README.md': `Built by the ${PHRASE}. It is a v0.x release.\n` })
  assert.equal(r.status, 0, r.out)
  assert.match(r.out, /public check: clean/)
})

check('exact phrase in RELEASE_NOTES.md passes', () => {
  const r = run({ 'RELEASE_NOTES.md': `${PHRASE}\n` })
  assert.equal(r.status, 0, r.out)
})

check('exact phrase in any other file fails', () => {
  const r = run({ 'docs2/notes.md': `${PHRASE}\n` })
  assert.equal(r.status, 1, r.out)
  assert.match(r.out, /notes\.md/)
})

check('README.md in a subfolder is not the allowed file', () => {
  const r = run({ 'sub/README.md': `${PHRASE}\n` })
  assert.equal(r.status, 1, r.out)
})

check('bare name in README.md fails', () => {
  const r = run({ 'README.md': `Made with ${NAME}.\n` })
  assert.equal(r.status, 1, r.out)
})

check('other case or form in README.md fails', () => {
  for (const text of [PHRASE.toLowerCase(), `${NAME} pipeline (${ACRO})`, `${NAME.toUpperCase()} Automatic Coding Pipeline (${ACRO})`]) {
    const r = run({ 'README.md': `${text}\n` })
    assert.equal(r.status, 1, `${text}: ${r.out}`)
  }
})

check('the phrase does not hide another forbidden word on the same README line', () => {
  const r = run({ 'README.md': `${PHRASE} and ${NAME} again\n` })
  assert.equal(r.status, 1, r.out)
})

check('bare acronym without the full phrase fails in README.md and elsewhere', () => {
  for (const [name, text] of [['README.md', `Built by ${ACRO}.\n`], ['notes.md', `${ACRO} pipeline\n`], ['README.md', `${PHRASE}; ${ACRO} again\n`]]) {
    const r = run({ [name]: text })
    assert.equal(r.status, 1, `${name}: ${r.out}`)
    assert.match(r.out, /UNEXPANDED/)
  }
})

check('identifiers that merely contain the letters pass', () => {
  const r = run({ 'src/a.ts': 'const public_' + ACRO.toLowerCase() + '_x = 1\n' })
  assert.equal(r.status, 0, r.out)
})

// NTFS cannot hold a colon in a file name, so this case is measured on POSIX hosts only.
if (process.platform === 'win32') console.log('NOT MEASURED a file whose name only starts with README.md: (win32: no colon file names)')
else check('a file whose name only starts with README.md: is not the allowed file', () => {
  const r = run({ 'README.md:other': `${PHRASE}\n`, 'RELEASE_NOTES.md:x': `${PHRASE}\n` })
  assert.equal(r.status, 1, r.out)
})

check('a regular file named node_modules or .git is scanned, not pruned', () => {
  for (const name of ['src/node_modules', 'src/.git']) {
    const r = run({ [name]: `${NAME}\n` })
    assert.equal(r.status, 1, `${name}: ${r.out}`)
  }
})

check('a node_modules directory is still pruned', () => {
  const r = run({ 'node_modules/pkg/readme.md': `${NAME}\n`, 'README.md': 'clean\n' })
  assert.equal(r.status, 0, r.out)
})

check('no disabled checks in this file', () => {
  const text = readFileSync(__filename, 'utf8')
  for (const word of ['.sk' + 'ip(', 'x' + 'fail', '.on' + 'ly(']) assert.ok(!text.includes(word), word)
})

if (failed) {
  console.log(`\n${failed} check(s) FAILED`)
  process.exit(1)
}
console.log('\npublic_macp_phrase check passed')
