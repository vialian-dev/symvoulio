// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// CARD R58 (cruel tester): the public export must be able to LIST every Playwright config it ships tests for. A test file that
// the export carries (tests/desktop/*.dspec.ts) but whose config is not exported cannot be discovered by anyone with the export.
// The committed tree is exported with tools/public-export/export.sh (as ct_publication4 does) and, inside the export,
// `npx playwright test -c <config> --list` must succeed and report «Total: N tests» with N >= 1, for the desktop config and
// for the r07b config. node_modules is the repo's own, linked into the export (no npm ci: this is about discovery, not install).
// Run: npx jiti tests/ct_r58_export_discovery.check.ts
import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { existsSync, mkdtempSync, rmSync, rmdirSync, symlinkSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

const root = resolve(__dirname, '..')
const posix = (p: string) => p.replace(/\\/g, '/')
const exportScript = join(root, 'tools', 'public-export', 'export.sh')

function findBash(): string {
  for (const c of [process.env.CT_BASH, 'C:/Program Files/Git/bin/bash.exe', 'bash'].filter(Boolean) as string[]) {
    const r = spawnSync(c, ['-c', 'echo ok'], { encoding: 'utf8' })
    if (r.status === 0 && r.stdout.trim() === 'ok') return c
  }
  throw new Error('no bash found (set CT_BASH)')
}

let failed = 0
function check(name: string, fn: () => void) {
  try { fn(); console.log(`PASS ${name}`) } catch (e) {
    failed++
    console.log(`FAIL ${name}\n     ${String((e as Error).message).split('\n').join('\n     ')}`)
  }
}

const parent = mkdtempSync(join(tmpdir(), 'ct-r58-'))
const dir = join(parent, 'export')
try {
  const env = { ...process.env } as Record<string, string | undefined>
  delete env.SHELLOPTS
  const exp = spawnSync(findBash(), ['-c', 'set +o igncr 2>/dev/null; exec bash "$@"', 'bash', posix(exportScript), posix(dir)], { cwd: root, encoding: 'utf8', env: env as NodeJS.ProcessEnv, maxBuffer: 64 * 1024 * 1024 })
  if (existsSync(join(root, 'node_modules'))) symlinkSync(join(root, 'node_modules'), join(dir, 'node_modules'), 'junction')

  check('test_r58_export_succeeds', () => {
    assert.equal(exp.status, 0, `export.sh exit ${exp.status}:\n${(exp.stdout + exp.stderr).slice(0, 1500)}`)
    assert.ok(existsSync(join(dir, 'node_modules')), 'no node_modules to link (npm ci in the repo first)')
  })

  for (const config of ['playwright.desktop.config.ts', 'playwright.desktop.r07b.config.ts']) {
    check(`test_r58_export_lists_tests_of_${config.replace(/[^a-z0-9]+/gi, '_')}`, () => {
      assert.ok(exp.status === 0, 'no export to list in')
      const r = spawnSync(`npx playwright test -c ${config} --list`, { cwd: dir, shell: true, encoding: 'utf8', timeout: 5 * 60 * 1000, maxBuffer: 64 * 1024 * 1024 })
      const text = (r.stdout ?? '') + (r.stderr ?? '')
      assert.equal(r.status, 0, `«npx playwright test -c ${config} --list» failed in the export, exit ${r.status}:\n${text.slice(0, 1200)}`)
      const m = /Total: (\d+) tests?/.exec(text)
      assert.ok(m && Number(m[1]) >= 1, `«${config} --list» lists no tests:\n${text.slice(-600)}`)
    })
  }
} finally {
  try { rmdirSync(join(dir, 'node_modules')) } catch { /* no link */ } // a junction: removes the link, not the target
  rmSync(parent, { recursive: true, force: true })
}
if (failed) { console.log(`${failed} check(s) failed`); process.exit(1) }
console.log('r58 export discovery check passed')
