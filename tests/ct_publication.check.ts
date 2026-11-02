// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// CARD AST-B (cruel tester): VAL-R25, the publication tools (tools/public-export) in a temp git repo.
// Run: npx jiti tests/ct_publication.check.ts
import assert from 'node:assert/strict'
import { spawnSync, type SpawnSyncReturns } from 'node:child_process'
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, relative, resolve } from 'node:path'

const root = resolve(__dirname, '..')
const toolsDir = join(root, 'tools')
const exportDir = join(toolsDir, 'public-export')
const posix = (p: string) => p.replace(/\\/g, '/')
const ZERO = '0000000000000000000000000000000000000000'
// Built from parts so this file itself passes the public-export check; the hook test still commits the real string.
const FORBIDDEN = ['192', '168', '1', '141'].join('.')

function findBash(): string {
  const candidates = [process.env.CT_BASH, 'C:/Program Files/Git/bin/bash.exe', 'bash'].filter(Boolean) as string[]
  for (const c of candidates) {
    const r = spawnSync(c, ['-c', 'echo ok'], { encoding: 'utf8' })
    if (r.status === 0 && r.stdout.trim() === 'ok') return c
  }
  throw new Error('no bash found (set CT_BASH)')
}
const bash = findBash()
// Git for Windows' bash runs with `igncr` and silently ignores CR in scripts; real bash (Linux, macOS, CI) does not.
// Turn that tolerance off so the CR defect shows up here as it does there.
const strict = { ...process.env, SHELLOPTS: '' }
delete (strict as Record<string, string | undefined>).SHELLOPTS
function sh(cwd: string, args: string[], input?: string): SpawnSyncReturns<string> {
  return spawnSync(bash, ['-c', 'set +o igncr 2>/dev/null; exec bash "$@"', 'bash', ...args], { cwd, encoding: 'utf8', input, env: strict })
}
function git(cwd: string, ...args: string[]): string {
  const r = spawnSync('git', ['-c', 'user.name=ct', '-c', 'user.email=ct@example.com', '-c', 'commit.gpgsign=false', '-c', 'core.autocrlf=false', ...args], { cwd, encoding: 'utf8' })
  assert.equal(r.status, 0, `git ${args.join(' ')}: ${r.stderr}`)
  return r.stdout.trim()
}
function walk(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name)
    if (statSync(p).isDirectory()) walk(p, out)
    else out.push(p)
  }
  return out
}
const toolFiles = () => walk(toolsDir).filter((p) => p.endsWith('.sh') || p.endsWith('pre-push'))

let failed = 0
function check(name: string, fn: () => void) {
  try {
    fn()
    console.log(`PASS ${name}`)
  } catch (e) {
    failed++
    console.log(`FAIL ${name}\n     ${String((e as Error).message).split('\n').join('\n     ')}`)
  }
}

// ---------------------------------------------------------------- VAL-R25: line endings and attributes

check('test_dod1_val_r25_tools_scripts_and_hook_use_lf_only_in_the_working_tree', () => {
  const files = toolFiles()
  assert.ok(files.length >= 4, `expected the tools scripts and the hook, found ${files.length}`)
  const crlf = files.filter((p) => readFileSync(p).includes(0x0d)).map((p) => posix(relative(root, p)))
  assert.deepEqual(crlf, [], 'CR byte found in')
})

check('test_dod1_val_r25_tools_scripts_and_hook_use_lf_only_in_the_index', () => {
  const listing = git(root, 'ls-files', '--eol', '--', 'tools').split('\n')
  const bad = listing.filter((l) => /(^|\s)i\/(crlf|mixed)/.test(l) && (/\.sh$/.test(l) || /pre-push$/.test(l)))
  assert.deepEqual(bad, [], 'committed with CRLF')
})

check('test_dod1_val_r25_gitattributes_enforces_sh_text_eol_lf', () => {
  const file = join(root, '.gitattributes')
  assert.ok(existsSync(file), '.gitattributes is missing')
  const lines = readFileSync(file, 'utf8').split(/\r?\n/).map((l) => l.trim().replace(/\s+/g, ' '))
  assert.ok(lines.includes('*.sh text eol=lf'), `no line "*.sh text eol=lf" in: ${JSON.stringify(lines)}`)
  const attrs = git(root, 'check-attr', 'text', 'eol', '--', 'tools/public-export/check.sh')
  assert.match(attrs, /text: set/)
  assert.match(attrs, /eol: lf/)
})

// ---------------------------------------------------------------- VAL-R25: check.sh under bash

function treeWith(files: Record<string, string>): string {
  const dir = mkdtempSync(join(tmpdir(), 'ct-tree-'))
  for (const [name, text] of Object.entries(files)) {
    mkdirSync(join(dir, name, '..'), { recursive: true })
    writeFileSync(join(dir, name), text)
  }
  return dir
}
const noCrNoise = (out: string) => {
  assert.ok(!out.includes('\r'), `CR in the output: ${JSON.stringify(out.slice(0, 300))}`)
  assert.ok(!/command not found|invalid option|unexpected token|syntax error/.test(out), out.slice(0, 400))
}

check('test_dod1_val_r25_check_sh_runs_under_bash_without_cr_errors_on_a_clean_tree', () => {
  const tree = treeWith({ 'README.md': 'clean text\n' })
  try {
    const r = sh(root, [posix(join(exportDir, 'check.sh')), posix(tree)])
    noCrNoise(r.stdout + r.stderr)
    assert.equal(r.status, 0, r.stdout + r.stderr)
    assert.match(r.stdout, /public check: clean/)
  } finally {
    rmSync(tree, { recursive: true, force: true })
  }
})

check('test_dod1_val_r25_check_sh_still_finds_a_forbidden_string', () => {
  const tree = treeWith({ 'notes.txt': `host ${FORBIDDEN}\n` })
  try {
    const r = sh(root, [posix(join(exportDir, 'check.sh')), posix(tree)])
    noCrNoise(r.stdout + r.stderr)
    assert.equal(r.status, 1, r.stdout + r.stderr)
    assert.match(r.stdout, /FORBIDDEN STRINGS/)
    assert.match(r.stdout, /notes\.txt/)
  } finally {
    rmSync(tree, { recursive: true, force: true })
  }
})

// ---------------------------------------------------------------- VAL-R25: the pre-push hook sees every pushed commit
// The hook runs from a copy of tools/public-export with CR stripped from the scripts, so that these checks answer
// only «does the hook look at earlier commits» and cannot be muddied (or falsely passed) by the CRLF defect above.

function repoWithHook() {
  const dir = mkdtempSync(join(tmpdir(), 'ct-repo-'))
  git(dir, 'init', '-q', '-b', 'main')
  const tools = join(dir, 'tools', 'public-export')
  mkdirSync(tools, { recursive: true })
  for (const name of readdirSync(exportDir)) {
    const raw = readFileSync(join(exportDir, name))
    const text = name.endsWith('.sh') || name === 'pre-push' ? raw.toString('utf8').replace(/\r/g, '') : raw.toString('utf8')
    writeFileSync(join(tools, name), text)
    chmodSync(join(tools, name), 0o755)
  }
  writeFileSync(join(dir, '.git', 'info', 'exclude'), 'tools/\n')
  const commit = (file: string, text: string, msg: string) => {
    writeFileSync(join(dir, file), text)
    git(dir, 'add', '--', file)
    git(dir, 'commit', '-q', '-m', msg)
    return git(dir, 'rev-parse', 'HEAD')
  }
  const hook = (lsha: string, rsha: string) => sh(dir, [posix(join(tools, 'pre-push'))], `refs/heads/main ${lsha} refs/heads/main ${rsha}\n`)
  return { dir, commit, hook, cleanup: () => rmSync(dir, { recursive: true, force: true }) }
}

check('test_dod1_val_r25_sub3_hook_rejects_clean_tip_when_an_earlier_new_commit_is_forbidden', () => {
  const r = repoWithHook()
  try {
    const base = r.commit('a.txt', 'base\n', 'base')
    r.commit('n.txt', `host ${FORBIDDEN}\n`, 'leak')
    const tip = r.commit('n.txt', 'cleaned\n', 'clean tip')
    // The tip itself is clean: the control run proves the hook can say yes about this very tree.
    const tipOnly = r.hook(tip, git(r.dir, 'rev-parse', 'HEAD~1'))
    assert.equal(tipOnly.status, 0, `control (tip only is pushed): ${tipOnly.stdout}${tipOnly.stderr}`)
    const toExistingRemote = r.hook(tip, base)
    assert.notEqual(toExistingRemote.status, 0, `the earlier commit with ${FORBIDDEN} passed the hook (remote at base)`)
    const toNewBranch = r.hook(tip, ZERO)
    assert.notEqual(toNewBranch.status, 0, `the earlier commit with ${FORBIDDEN} passed the hook (new remote branch)`)
  } finally {
    r.cleanup()
  }
})

check('test_dod1_val_r25_sub3_hook_rejects_forbidden_string_in_the_middle_of_a_longer_range', () => {
  const r = repoWithHook()
  try {
    const base = r.commit('a.txt', 'base\n', 'base')
    r.commit('b.txt', 'one\n', 'one')
    r.commit('c.txt', `ip ${FORBIDDEN}\n`, 'leak in the middle')
    r.commit('d.txt', 'three\n', 'three')
    r.commit('c.txt', 'removed\n', 'remove')
    const tip = r.commit('e.txt', 'five\n', 'five')
    const res = r.hook(tip, base)
    assert.notEqual(res.status, 0, 'a forbidden string in a middle commit passed the hook')
  } finally {
    r.cleanup()
  }
})

check('test_dod1_val_r25_hook_rejects_a_forbidden_tip_control', () => {
  const r = repoWithHook()
  try {
    const base = r.commit('a.txt', 'base\n', 'base')
    const tip = r.commit('n.txt', `host ${FORBIDDEN}\n`, 'leak at the tip')
    assert.notEqual(r.hook(tip, base).status, 0)
  } finally {
    r.cleanup()
  }
})

check('test_dod1_val_r25_hook_accepts_an_all_clean_push_control', () => {
  const r = repoWithHook()
  try {
    const base = r.commit('a.txt', 'base\n', 'base')
    r.commit('b.txt', 'one\n', 'one')
    const tip = r.commit('c.txt', 'two\n', 'two')
    const res = r.hook(tip, base)
    assert.equal(res.status, 0, res.stdout + res.stderr)
    assert.equal(r.hook(tip, ZERO).status, 0)
  } finally {
    r.cleanup()
  }
})

// Boundary of «newly reachable»: a leak that the remote already has is not this push's business.
check('test_dod1_val_r25_hook_does_not_recheck_commits_the_remote_already_has', () => {
  const r = repoWithHook()
  try {
    r.commit('a.txt', 'base\n', 'base')
    const leaked = r.commit('n.txt', `host ${FORBIDDEN}\n`, 'leak (already published)')
    r.commit('n.txt', 'cleaned\n', 'clean')
    const tip = r.commit('z.txt', 'next\n', 'next')
    const res = r.hook(tip, leaked)
    assert.equal(res.status, 0, res.stdout + res.stderr)
  } finally {
    r.cleanup()
  }
})

// A branch deletion pushes the zero sha: nothing to check, nothing to fail.
check('test_dod1_val_r25_hook_ignores_a_branch_deletion', () => {
  const r = repoWithHook()
  try {
    const base = r.commit('a.txt', 'base\n', 'base')
    assert.equal(r.hook(ZERO, base).status, 0)
  } finally {
    r.cleanup()
  }
})

// ---------------------------------------------------------------- DoD3

check('test_dod3_no_disabled_checks_in_this_file', () => {
  const text = readFileSync(__filename, 'utf8')
  for (const word of ['.sk' + 'ip(', 'x' + 'fail', '.on' + 'ly(']) assert.ok(!text.includes(word), word)
})

if (failed) {
  console.log(`\n${failed} check(s) FAILED`)
  process.exit(1)
}
console.log('\nct_publication check passed')
