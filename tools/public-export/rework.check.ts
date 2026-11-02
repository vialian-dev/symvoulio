// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { cpSync, mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { resolve, join } from 'node:path'
const source = resolve('tools/public-export')
const tmp = mkdtempSync(join(tmpdir(), 'export-regression-'))
const bash = process.platform === 'win32' ? 'C:/Program Files/Git/bin/bash.exe' : 'bash'
const run = (cwd: string, exe: string, args: string[]) => spawnSync(exe, args, { cwd, encoding: 'utf8', env: { ...process.env, GIT_CONFIG_COUNT: '3', GIT_CONFIG_KEY_0: 'user.name', GIT_CONFIG_VALUE_0: 'Test', GIT_CONFIG_KEY_1: 'user.email', GIT_CONFIG_VALUE_1: 'test@example.com', GIT_CONFIG_KEY_2: 'commit.gpgsign', GIT_CONFIG_VALUE_2: 'false' } })
const git = (cwd: string, ...args: string[]) => { const r = run(cwd, 'git', args); assert.equal(r.status, 0, r.stderr); return r.stdout }
let failures = 0
function test(name: string, fn: () => void) { try { fn(); console.log(`PASS ${name}`) } catch (e) { failures++; console.log(`FAIL ${name}: ${e}`) } }
try {
  const repo = join(tmp, 'repo'); mkdirSync(repo)
  cpSync(source, join(repo, 'tools/public-export'), { recursive: true })
  writeFileSync(join(repo, 'tools/public-export/include.txt'), 'src\ntools/public-export\n')
  mkdirSync(join(repo, 'src')); writeFileSync(join(repo, 'src/document.txt'), 'committed')
  git(repo, 'init'); git(repo, 'add', '.'); git(repo, 'commit', '-m', 'initial')
  writeFileSync(join(repo, 'src/document.txt'), 'uncommitted')
  writeFileSync(join(repo, 'src/private-draft.txt'), 'private draft')
  test('default export uses HEAD and excludes untracked and modified content', () => {
    const out = join(tmp, 'export')
    const r = run(repo, bash, ['tools/public-export/export.sh', out.replaceAll('\\', '/')])
    assert.equal(r.status, 0, r.stdout + r.stderr)
    assert.equal(existsSync(join(out, 'src/private-draft.txt')), false)
    assert.equal(readFileSync(join(out, 'src/document.txt'), 'utf8'), 'committed')
  })
  test('capitalised internal names fail the public scan', () => {
    const planted = join(tmp, 'capitalised'); mkdirSync(planted)
    for (const name of ["Ast" + "ra's", 'Git' + 'Lab']) {
      writeFileSync(join(planted, 'document.txt'), name)
      const result = run(repo, bash, [join(source, 'check.sh').replaceAll('\\', '/'), planted.replaceAll('\\', '/')])
      assert.notEqual(result.status, 0, `${name} must fail the scan`)
      assert.match(result.stdout, /public check: FAILED/)
    }
  })
  test('public comment cleanup preserves assertions, titles, and template contents', () => {
    const dir = join(tmp, 'comments'); mkdirSync(join(dir, 'tests'), { recursive: true })
    const id = ['D', 'SY', '123'].join('-')
    const file = join(dir, 'tests/ct_v02_field.check.ts')
    const runtime = `const text = \`\n// ${id} stays inside a template\n\`;\ntest('${id} stays in a title', () => assert.equal(text, '${id}'));\n`
    writeFileSync(file, `// ${id} private metadata\n// useful contract explanation\n${runtime}`)
    const result = run(repo, process.execPath, [join(source, 'sanitize-comments.mjs'), dir])
    assert.equal(result.status, 0, result.stdout + result.stderr)
    const clean = readFileSync(file, 'utf8')
    assert.equal(clean, `\n// useful contract explanation\n${runtime}`)
    // Runtime leaks must still be rejected by the unchanged scanner.
    const scan = run(repo, bash, [join(source, 'check.sh').replaceAll('\\', '/'), dir.replaceAll('\\', '/')])
    assert.notEqual(scan.status, 0)
    assert.match(scan.stdout, /FORBIDDEN STRINGS/)
  })
  test('fresh orphan history scans two commits and rejects an earlier leak', () => {
    const script = join(source, 'history-check.sh').replaceAll('\\', '/')
    const clean = join(tmp, 'clean'); mkdirSync(clean); writeFileSync(join(clean, 'document.txt'), 'clean')
    const good = run(repo, bash, [script, clean.replaceAll('\\', '/')])
    console.log(good.stdout.trim()); assert.equal(good.status, 0, good.stderr)
    assert.match(good.stdout, /history-check: scanned 2 reachable commits/)
    const bad = run(repo, bash, [script, clean.replaceAll('\\', '/'), '--positive-control'])
    console.log(bad.stdout.trim()); assert.notEqual(bad.status, 0)
    assert.match(bad.stdout, /FORBIDDEN STRINGS/)
    assert.match(bad.stdout, /history-check: scanned 2 reachable commits/)
  })
} finally { rmSync(tmp, { recursive: true, force: true }) }
if (failures) process.exit(1)
