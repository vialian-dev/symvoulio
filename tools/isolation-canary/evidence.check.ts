// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { resolve, join } from 'node:path'
import { spawnSync } from 'node:child_process'
const base = tmpdir()
mkdirSync(base, { recursive: true })
const parent = mkdtempSync(`${base}/canary-raw-`)
const bin = join(parent, 'bin'), repo = join(parent, 'repo'), out = join(parent, 'evidence')
mkdirSync(bin); mkdirSync(repo)
const bash = 'C:/Program Files/Git/bin/bash.exe'
const script = resolve('tools/isolation-canary/canary.sh').replace(/\\/g, '/')
function stub(name: string, body: string) { writeFileSync(join(bin, name), `#!/usr/bin/env bash\n${body}\n`) }
try {
  for (const args of [['init', '-q'], ['-c', 'user.name=fixture', '-c', 'user.email=fixture@example.com', 'commit', '--allow-empty', '-qm', 'fixture']]) {
    assert.equal(spawnSync('git', args, { cwd: repo }).status, 0)
  }
  stub('claude', 'echo fake'); stub('codex', 'echo fake')
  stub('cygpath', 'shift; echo "$1"')
  stub('smoke-fixture', `mkdir -p "$1/output/audit/provider_output"
secret=$(grep -o '[^ ]*secret_canary[.]txt' "$2" | head -1)
token=$(cat "$secret")
printf '%s\\n' "malformed provider body $token" > "$5"
echo '{"state":"paused","lastError":{"kind":"malformed","message":"not JSON"}}'
`)
  stub('cargo', 'mkdir -p target/debug/examples; cp "$(dirname "$0")/smoke-fixture" target/debug/examples/smoke.exe; chmod +x target/debug/examples/smoke.exe')
  const result = spawnSync(bash, [script, out.replace(/\\/g, '/')], { cwd: repo, encoding: 'utf8', env: { ...process.env, PATH: `${bin};${process.env.PATH}`, CARGO_TARGET_DIR: '' } })
  assert.equal(result.status, 1, result.stderr + result.stdout)
  assert.match(result.stdout, /token_hits: [1-9]/)
  assert.match(result.stdout, /successful_completion: 0/)
  for (const suffix of ['', '-codex']) {
    assert.match(readFileSync(join(out, `smoke_raw${suffix}.log`), 'utf8'), /malformed provider body CANARY-/)
    assert.doesNotMatch(readFileSync(join(out, `smoke_events${suffix}.log`), 'utf8'), /CANARY-/)
  }
  const cases = [
    { name: 'completed', state: 'completed', stopReason: 'gate_max_rounds', roundsDone: 1, rounds: [{ round: 1 }], pass: true },
    { name: 'gate stopped full round', state: 'stopped', stopReason: 'gate_human_review_required', roundsDone: 1, rounds: [{ round: 1 }], pass: true },
    { name: 'malformed paused', state: 'paused', lastError: { kind: 'malformed' }, roundsDone: 1, rounds: [{ round: 1 }], pass: false },
    { name: 'gate stopped no round', state: 'stopped', stopReason: 'gate_human_review_required', roundsDone: 0, rounds: [], pass: false },
    { name: 'completed no round', state: 'completed', roundsDone: 0, rounds: [], pass: false },
  ]
  for (const [i, fixture] of cases.entries()) {
    stub('smoke-fixture', `printf '%s\\n' 'safe provider body' > "$5"
cat <<'JSON'
${JSON.stringify(fixture)}
JSON`)
    const evidence = join(parent, `case-${i}`)
    const checked = spawnSync(bash, [script, evidence.replace(/\\/g, '/')], { cwd: repo, encoding: 'utf8', env: { ...process.env, PATH: `${bin};${process.env.PATH}`, CARGO_TARGET_DIR: '' } })
    assert.equal(checked.status, fixture.pass ? 0 : 1, `${fixture.name}: ${checked.stderr}${checked.stdout}`)
    assert.match(checked.stdout, new RegExp(`successful_completion: ${fixture.pass ? 1 : 0}`), fixture.name)
  }
  console.log('PASS terminal state and full structured round canary criteria (5 cases)')
  console.log('PASS raw-only canary leak detection and malformed-output evidence for both assignments')
} finally { rmSync(parent, { recursive: true, force: true }) }
